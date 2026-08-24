use std::path::PathBuf;

use gtl_models::{
    diffs::{CommitId, DiffLineCount, ExcludedExtensions},
    git::{GitHead, GitRevision},
    paths::{AbsoluteFilePath, ProjectName, RepositoryRelativePath},
    timestamps::MachineTimestamp,
    viewer::{
        HistoryPage, HistoryPageCount, HistoryPageNumber, HistoryPagePosition, HistoryRenderCount,
        RenderHistoryId, ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId,
        ViewerVersion,
    },
};
use gtl_wire::{
    v1,
    viewer::{
        GetViewerHistoryCopy, ListViewerCommits, ListViewerHistory, OpenViewerDiffFile,
        OpenViewerHistory, SelectViewerCommit, SetViewerPreference, StreamViewerRows,
        ViewerActiveState, ViewerActiveView, ViewerAppliedExclusions, ViewerCodeLine,
        ViewerCodeSpan, ViewerCommandLine, ViewerCommitCursor, ViewerCommitPage,
        ViewerCommitSelection, ViewerCommitSummary, ViewerDiffDensity, ViewerDiffExclusions,
        ViewerDiffLayout, ViewerFailureCode, ViewerFeedback, ViewerFileFailureCode,
        ViewerFileStatus, ViewerFileSummary, ViewerFooter, ViewerHistoryCopyPayload,
        ViewerHistoryCursor, ViewerHistoryEntry, ViewerHistoryPage, ViewerPreferences,
        ViewerProjectDiffExclusions, ViewerRecipeKind, ViewerRenderOptions, ViewerRowEvent,
        ViewerRowStreamItem, ViewerShell, ViewerSplitCell, ViewerSplitRow, ViewerStateChanged,
        ViewerSyntaxClass, ViewerTab, ViewerTabKind, ViewerTabRequest, ViewerTabState, ViewerTheme,
        ViewerUnifiedRow, ViewerUnifiedSourceRow, ViewerUserSettings, ViewerViewIdentity,
    },
};

use super::ViewerClientError;

macro_rules! viewer_tab_codecs {
    ($($encode:ident => $request:ident; $decode:ident <= $response:ident;)+) => {
        $(
            pub(super) fn $encode(request: ViewerTabRequest) -> v1::$request {
                v1::$request {
                    tab_id: request.tab_id.into(),
                }
            }

            pub(super) fn $decode(
                response: v1::$response,
            ) -> Result<ViewerShell, ViewerClientError> {
                decode_viewer_shell(required(response.shell)?)
            }
        )+
    };
}

macro_rules! viewer_shell_response_decoders {
    ($($decode:ident <= $response:ident;)+) => {
        $(
            pub(super) fn $decode(
                response: v1::$response,
            ) -> Result<ViewerShell, ViewerClientError> {
                decode_viewer_shell(required(response.shell)?)
            }
        )+
    };
}

viewer_tab_codecs! {
    encode_activate_viewer_tab_request => ActivateViewerTabRequest;
    decode_activate_viewer_tab_response <= ActivateViewerTabResponse;
    encode_close_viewer_tab_request => CloseViewerTabRequest;
    decode_close_viewer_tab_response <= CloseViewerTabResponse;
    encode_refresh_viewer_tab_request => RefreshViewerTabRequest;
    decode_refresh_viewer_tab_response <= RefreshViewerTabResponse;
    encode_delete_live_viewer_tab_request => DeleteLiveViewerTabRequest;
    decode_delete_live_viewer_tab_response <= DeleteLiveViewerTabResponse;
    encode_clear_viewer_commit_selection_request => ClearViewerCommitSelectionRequest;
    decode_clear_viewer_commit_selection_response <= ClearViewerCommitSelectionResponse;
}

viewer_shell_response_decoders! {
    decode_get_viewer_shell_response <= GetViewerShellResponse;
    decode_select_viewer_commit_response <= SelectViewerCommitResponse;
    decode_set_viewer_preference_response <= SetViewerPreferenceResponse;
    decode_open_viewer_history_response <= OpenViewerHistoryResponse;
}

pub(super) fn encode_select_viewer_commit_request(
    request: SelectViewerCommit,
) -> v1::SelectViewerCommitRequest {
    v1::SelectViewerCommitRequest {
        tab_id: request.tab_id.into(),
        commit_id: request.id.to_string(),
    }
}

pub(super) fn encode_set_viewer_preference_request(
    request: SetViewerPreference,
) -> v1::SetViewerPreferenceRequest {
    let preference = match request {
        SetViewerPreference::Layout(layout) => {
            v1::set_viewer_preference_request::Preference::Layout(
                encode_viewer_diff_layout(layout) as i32
            )
        }
        SetViewerPreference::Density(density) => {
            v1::set_viewer_preference_request::Preference::Density(encode_viewer_diff_density(
                density,
            ) as i32)
        }
        SetViewerPreference::Theme(theme) => {
            v1::set_viewer_preference_request::Preference::Theme(encode_viewer_theme(theme) as i32)
        }
    };
    v1::SetViewerPreferenceRequest {
        preference: Some(preference),
    }
}

pub(super) fn encode_list_viewer_commits_request(
    request: ListViewerCommits,
) -> v1::ListViewerCommitsRequest {
    v1::ListViewerCommitsRequest {
        identity: Some(encode_viewer_view_identity(request.identity)),
        cursor: request.cursor.map(ViewerCommitCursor::into_inner),
    }
}

pub(super) fn decode_list_viewer_commits_response(
    response: v1::ListViewerCommitsResponse,
) -> Result<ViewerCommitPage, ViewerClientError> {
    Ok(ViewerCommitPage {
        identity: decode_viewer_view_identity(required(response.identity)?)?,
        commits: response
            .commits
            .into_iter()
            .map(decode_viewer_commit_summary)
            .collect::<Result<Vec<_>, _>>()?,
        next_cursor: response.next_cursor.map(ViewerCommitCursor::new),
    })
}

pub(super) fn encode_list_viewer_history_request(
    request: ListViewerHistory,
) -> Result<v1::ListViewerHistoryRequest, ViewerClientError> {
    use v1::list_viewer_history_request::Cursor;

    let cursor = match request.cursor {
        ViewerHistoryCursor::Newest => Cursor::Newest(v1::Empty {}),
        ViewerHistoryCursor::OlderThan { render_id, page } => {
            Cursor::OlderThan(encode_viewer_history_position(render_id, page)?)
        }
        ViewerHistoryCursor::NewerThan { render_id, page } => {
            Cursor::NewerThan(encode_viewer_history_position(render_id, page)?)
        }
        ViewerHistoryCursor::Oldest => Cursor::Oldest(v1::Empty {}),
    };
    Ok(v1::ListViewerHistoryRequest {
        cursor: Some(cursor),
    })
}

pub(super) fn decode_list_viewer_history_response(
    response: v1::ListViewerHistoryResponse,
) -> Result<ViewerHistoryPage, ViewerClientError> {
    let entries = response
        .entries
        .into_iter()
        .map(decode_viewer_history_entry)
        .collect::<Result<Vec<_>, _>>()?;
    let position = match response.position {
        Some(position) => HistoryPagePosition::Page(
            HistoryPage::new(
                HistoryPageNumber::try_new(position.number)
                    .map_err(|_| ViewerClientError::Internal)?,
                HistoryPageCount::try_new(position.count)
                    .map_err(|_| ViewerClientError::Internal)?,
            )
            .map_err(|_| ViewerClientError::Internal)?,
        ),
        None if entries.is_empty() => HistoryPagePosition::Empty,
        None => return Err(ViewerClientError::Internal),
    };
    Ok(ViewerHistoryPage {
        entries,
        total_count: HistoryRenderCount::new(response.total_count),
        position,
        has_newer: response.has_newer,
        has_older: response.has_older,
    })
}

pub(super) fn encode_open_viewer_history_request(
    request: OpenViewerHistory,
) -> Result<v1::OpenViewerHistoryRequest, ViewerClientError> {
    Ok(v1::OpenViewerHistoryRequest {
        render_id: encode_render_history_id(request.render_id)?,
    })
}

pub(super) fn encode_get_viewer_history_copy_request(
    request: GetViewerHistoryCopy,
) -> Result<v1::GetViewerHistoryCopyRequest, ViewerClientError> {
    Ok(v1::GetViewerHistoryCopyRequest {
        render_id: encode_render_history_id(request.render_id)?,
    })
}

pub(super) fn decode_get_viewer_history_copy_response(
    response: v1::GetViewerHistoryCopyResponse,
) -> ViewerHistoryCopyPayload {
    ViewerHistoryCopyPayload {
        json: response.json,
    }
}

pub(super) fn decode_get_viewer_settings_response(
    response: v1::GetViewerSettingsResponse,
) -> Result<ViewerUserSettings, ViewerClientError> {
    let exclusions = required(response.diff_exclusions)?;
    Ok(ViewerUserSettings {
        configuration_path: response.configuration_path,
        configured_theme: response
            .configured_theme
            .map(decode_viewer_theme)
            .transpose()?,
        effective_theme: decode_viewer_theme(response.effective_theme)?,
        render_options: decode_viewer_render_options(required(response.render_options)?)?,
        push_confirmation_required: response.push_confirmation_required,
        diff_exclusions: ViewerDiffExclusions {
            default_extensions: ExcludedExtensions::new(exclusions.default_extensions),
            projects: exclusions
                .projects
                .into_iter()
                .map(|project| {
                    Ok(ViewerProjectDiffExclusions {
                        project_name: ProjectName::try_new(project.project_name)
                            .map_err(|_| ViewerClientError::Internal)?,
                        extensions: ExcludedExtensions::new(project.extensions),
                    })
                })
                .collect::<Result<Vec<_>, ViewerClientError>>()?,
        },
    })
}

pub(super) fn encode_open_viewer_diff_file_request(
    request: OpenViewerDiffFile,
) -> v1::OpenViewerDiffFileRequest {
    v1::OpenViewerDiffFileRequest {
        identity: Some(encode_viewer_view_identity(request.identity)),
        file_id: request.file.as_str().to_owned(),
    }
}

pub(super) fn encode_stream_viewer_rows_request(
    request: StreamViewerRows,
) -> v1::StreamViewerRowsRequest {
    v1::StreamViewerRowsRequest {
        identity: Some(encode_viewer_view_identity(request.identity)),
        file_id: request.file.map(|file| file.as_str().to_owned()),
    }
}

pub(super) fn decode_stream_viewer_rows_response(
    response: v1::StreamViewerRowsResponse,
) -> Result<ViewerRowStreamItem, ViewerClientError> {
    Ok(ViewerRowStreamItem {
        identity: decode_viewer_view_identity(required(response.identity)?)?,
        sequence: response.sequence,
        event: decode_viewer_row_event(required(response.event)?)?,
    })
}

pub(super) fn decode_watch_viewer_response(
    response: v1::WatchViewerResponse,
) -> ViewerStateChanged {
    ViewerStateChanged {
        version: ViewerVersion::new(response.version),
    }
}

fn decode_viewer_shell(shell: v1::ViewerShell) -> Result<ViewerShell, ViewerClientError> {
    let preferences = required(shell.preferences)?;
    Ok(ViewerShell {
        version: ViewerVersion::new(shell.version),
        tabs: shell
            .tabs
            .into_iter()
            .map(decode_viewer_tab)
            .collect::<Result<Vec<_>, _>>()?,
        active: decode_viewer_active_state(required(shell.active)?)?,
        preferences: ViewerPreferences {
            theme: decode_viewer_theme(preferences.theme)?,
            render_options: decode_viewer_render_options(required(preferences.render_options)?)?,
        },
        feedback: shell.feedback.map(decode_viewer_feedback).transpose()?,
    })
}

fn decode_viewer_tab(tab: v1::ViewerTab) -> Result<ViewerTab, ViewerClientError> {
    Ok(ViewerTab {
        id: ViewerTabId::try_new(tab.id).map_err(|_| ViewerClientError::Internal)?,
        label: tab.label,
        kind: match v1::ViewerTabKind::try_from(tab.kind) {
            Ok(v1::ViewerTabKind::Snapshot) => ViewerTabKind::Snapshot,
            Ok(v1::ViewerTabKind::Live) => ViewerTabKind::Live,
            Ok(v1::ViewerTabKind::Unspecified) | Err(_) => {
                return Err(ViewerClientError::Internal);
            }
        },
        state: match v1::ViewerTabState::try_from(tab.state) {
            Ok(v1::ViewerTabState::Pending) => ViewerTabState::Pending,
            Ok(v1::ViewerTabState::Ready) => ViewerTabState::Ready,
            Ok(v1::ViewerTabState::Broken) => ViewerTabState::Broken,
            Ok(v1::ViewerTabState::Error) => ViewerTabState::Error,
            Ok(v1::ViewerTabState::Unspecified) | Err(_) => {
                return Err(ViewerClientError::Internal);
            }
        },
    })
}

fn decode_viewer_active_state(
    active: v1::ViewerActiveState,
) -> Result<ViewerActiveState, ViewerClientError> {
    use v1::viewer_active_state::State;

    match required(active.state)? {
        State::Empty(_) => Ok(ViewerActiveState::Empty),
        State::Pending(state) => Ok(ViewerActiveState::Pending {
            tab_id: ViewerTabId::try_new(state.tab_id).map_err(|_| ViewerClientError::Internal)?,
        }),
        State::Broken(state) => decode_viewer_failure_state(state, true),
        State::Error(state) => decode_viewer_failure_state(state, false),
        State::Ready(state) => Ok(ViewerActiveState::Ready {
            view: Box::new(decode_viewer_active_view(required(state.view)?)?),
        }),
    }
}

fn decode_viewer_failure_state(
    failure: v1::ViewerFailureState,
    broken: bool,
) -> Result<ViewerActiveState, ViewerClientError> {
    let tab_id = ViewerTabId::try_new(failure.tab_id).map_err(|_| ViewerClientError::Internal)?;
    let code = match v1::ViewerFailureCode::try_from(failure.code) {
        Ok(v1::ViewerFailureCode::RepositoryDirectoryNotFound) => {
            ViewerFailureCode::RepositoryDirectoryNotFound
        }
        Ok(v1::ViewerFailureCode::RepositoryDirectoryNotGitRepository) => {
            ViewerFailureCode::RepositoryDirectoryNotGitRepository
        }
        Ok(v1::ViewerFailureCode::SourceUnavailable) => ViewerFailureCode::SourceUnavailable,
        Ok(v1::ViewerFailureCode::RenderFailed) => ViewerFailureCode::RenderFailed,
        Ok(v1::ViewerFailureCode::Unspecified) | Err(_) => {
            return Err(ViewerClientError::Internal);
        }
    };
    Ok(if broken {
        ViewerActiveState::Broken {
            tab_id,
            code,
            message: failure.message,
        }
    } else {
        ViewerActiveState::Error {
            tab_id,
            code,
            message: failure.message,
        }
    })
}

fn decode_viewer_active_view(
    view: v1::ViewerActiveView,
) -> Result<ViewerActiveView, ViewerClientError> {
    Ok(ViewerActiveView {
        identity: decode_viewer_view_identity(required(view.identity)?)?,
        title: view.title,
        repository_name: ProjectName::try_new(view.repository_name)
            .map_err(|_| ViewerClientError::Internal)?,
        branch: GitHead::try_from(view.branch).map_err(|_| ViewerClientError::Internal)?,
        upstream: GitRevision::try_new(view.upstream).map_err(|_| ViewerClientError::Internal)?,
        command: decode_viewer_command_line(required(view.command)?),
        files: view
            .files
            .into_iter()
            .map(decode_viewer_file_summary)
            .collect::<Result<Vec<_>, _>>()?,
        commits_label: view.commits_label,
        commit_count: usize::try_from(view.commit_count)
            .map_err(|_| ViewerClientError::Internal)?,
        commits: Vec::new(),
        commit_selection: decode_viewer_commit_selection(required(view.commit_selection)?)?,
        footer: ViewerFooter {
            command: required(view.footer)?.command,
        },
        exclusions: view
            .exclusions
            .map(decode_viewer_applied_exclusions)
            .transpose()?,
    })
}

fn decode_viewer_file_summary(
    file: v1::ViewerFileSummary,
) -> Result<ViewerFileSummary, ViewerClientError> {
    Ok(ViewerFileSummary {
        id: file
            .id
            .try_into()
            .map_err(|_| ViewerClientError::Internal)?,
        path: RepositoryRelativePath::try_new(PathBuf::from(file.path))
            .map_err(|_| ViewerClientError::Internal)?,
        absolute_path: AbsoluteFilePath::try_new(PathBuf::from(file.absolute_path))
            .map_err(|_| ViewerClientError::Internal)?,
        anchor_id: file.anchor_id,
        added: DiffLineCount::new(u64::from(file.added)),
        removed: DiffLineCount::new(u64::from(file.removed)),
        status: match v1::ViewerFileStatus::try_from(file.status) {
            Ok(v1::ViewerFileStatus::Added) => ViewerFileStatus::Added,
            Ok(v1::ViewerFileStatus::Deleted) => ViewerFileStatus::Deleted,
            Ok(v1::ViewerFileStatus::Renamed) => ViewerFileStatus::Renamed,
            Ok(v1::ViewerFileStatus::Modified) => ViewerFileStatus::Modified,
            Ok(v1::ViewerFileStatus::Unspecified) | Err(_) => {
                return Err(ViewerClientError::Internal);
            }
        },
        can_open_in_editor: file.can_open_in_editor,
        initially_expanded: file.initially_expanded,
    })
}

fn decode_viewer_command_line(command: v1::ViewerCommandLine) -> ViewerCommandLine {
    ViewerCommandLine {
        lead: command.lead,
        range: command.range,
        trail: command.trail,
    }
}

fn decode_viewer_commit_selection(
    selection: v1::ViewerCommitSelection,
) -> Result<ViewerCommitSelection, ViewerClientError> {
    let commit_id = || {
        selection
            .commit_id
            .clone()
            .ok_or(ViewerClientError::Internal)?
            .try_into()
            .map_err(|_| ViewerClientError::Internal)
    };
    match v1::ViewerCommitSelectionState::try_from(selection.state) {
        Ok(v1::ViewerCommitSelectionState::None) => Ok(ViewerCommitSelection::None),
        Ok(v1::ViewerCommitSelectionState::Pending) => {
            Ok(ViewerCommitSelection::Pending { id: commit_id()? })
        }
        Ok(v1::ViewerCommitSelectionState::Ready) => {
            Ok(ViewerCommitSelection::Ready { id: commit_id()? })
        }
        Ok(v1::ViewerCommitSelectionState::Error) => Ok(ViewerCommitSelection::Error {
            id: commit_id()?,
            message: selection.message.ok_or(ViewerClientError::Internal)?,
        }),
        Ok(v1::ViewerCommitSelectionState::Unspecified) | Err(_) => {
            Err(ViewerClientError::Internal)
        }
    }
}

fn decode_viewer_applied_exclusions(
    exclusions: v1::ViewerAppliedExclusions,
) -> Result<ViewerAppliedExclusions, ViewerClientError> {
    Ok(ViewerAppliedExclusions {
        extensions: ExcludedExtensions::new(exclusions.extensions),
        hidden_paths: exclusions
            .hidden_paths
            .into_iter()
            .map(|path| {
                RepositoryRelativePath::try_new(PathBuf::from(path))
                    .map_err(|_| ViewerClientError::Internal)
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn decode_viewer_feedback(
    feedback: v1::ViewerFeedback,
) -> Result<ViewerFeedback, ViewerClientError> {
    match v1::ViewerFeedbackKind::try_from(feedback.kind) {
        Ok(v1::ViewerFeedbackKind::TabClosed) => Ok(ViewerFeedback::TabClosed),
        Ok(v1::ViewerFeedbackKind::LiveViewDeleted) => Ok(ViewerFeedback::LiveViewDeleted),
        Ok(v1::ViewerFeedbackKind::SnapshotRecipesSkipped) => {
            Ok(ViewerFeedback::SnapshotRecipesSkipped {
                labels: feedback.labels,
            })
        }
        Ok(v1::ViewerFeedbackKind::Unspecified) | Err(_) => Err(ViewerClientError::Internal),
    }
}

fn decode_viewer_view_identity(
    identity: v1::ViewerViewIdentity,
) -> Result<ViewerViewIdentity, ViewerClientError> {
    Ok(ViewerViewIdentity {
        tab_id: ViewerTabId::try_new(identity.tab_id).map_err(|_| ViewerClientError::Internal)?,
        range_generation: ViewerRangeGeneration::new(identity.range_generation),
        selection_generation: ViewerSelectionGeneration::new(identity.selection_generation),
        render_options: decode_viewer_render_options(required(identity.render_options)?)?,
    })
}

fn encode_viewer_view_identity(identity: ViewerViewIdentity) -> v1::ViewerViewIdentity {
    v1::ViewerViewIdentity {
        tab_id: identity.tab_id.into(),
        range_generation: identity.range_generation.value(),
        selection_generation: identity.selection_generation.value(),
        render_options: Some(encode_viewer_render_options(identity.render_options)),
    }
}

fn decode_viewer_render_options(
    options: v1::ViewerRenderOptions,
) -> Result<ViewerRenderOptions, ViewerClientError> {
    Ok(ViewerRenderOptions {
        layout: match v1::ViewerDiffLayout::try_from(options.layout) {
            Ok(v1::ViewerDiffLayout::Unified) => ViewerDiffLayout::Unified,
            Ok(v1::ViewerDiffLayout::Split) => ViewerDiffLayout::Split,
            Ok(v1::ViewerDiffLayout::Unspecified) | Err(_) => {
                return Err(ViewerClientError::Internal);
            }
        },
        density: match v1::ViewerDiffDensity::try_from(options.density) {
            Ok(v1::ViewerDiffDensity::Compact) => ViewerDiffDensity::Compact,
            Ok(v1::ViewerDiffDensity::Full) => ViewerDiffDensity::Full,
            Ok(v1::ViewerDiffDensity::Unspecified) | Err(_) => {
                return Err(ViewerClientError::Internal);
            }
        },
    })
}

fn encode_viewer_render_options(options: ViewerRenderOptions) -> v1::ViewerRenderOptions {
    v1::ViewerRenderOptions {
        layout: encode_viewer_diff_layout(options.layout) as i32,
        density: encode_viewer_diff_density(options.density) as i32,
    }
}

const fn encode_viewer_diff_layout(layout: ViewerDiffLayout) -> v1::ViewerDiffLayout {
    match layout {
        ViewerDiffLayout::Unified => v1::ViewerDiffLayout::Unified,
        ViewerDiffLayout::Split => v1::ViewerDiffLayout::Split,
    }
}

const fn encode_viewer_diff_density(density: ViewerDiffDensity) -> v1::ViewerDiffDensity {
    match density {
        ViewerDiffDensity::Compact => v1::ViewerDiffDensity::Compact,
        ViewerDiffDensity::Full => v1::ViewerDiffDensity::Full,
    }
}

fn decode_viewer_theme(theme: i32) -> Result<ViewerTheme, ViewerClientError> {
    match v1::ViewerTheme::try_from(theme) {
        Ok(v1::ViewerTheme::Dark) => Ok(ViewerTheme::Dark),
        Ok(v1::ViewerTheme::Light) => Ok(ViewerTheme::Light),
        Ok(v1::ViewerTheme::Hearth) => Ok(ViewerTheme::Hearth),
        Ok(v1::ViewerTheme::Mirage) => Ok(ViewerTheme::Mirage),
        Ok(v1::ViewerTheme::Glacier) => Ok(ViewerTheme::Glacier),
        Ok(v1::ViewerTheme::Noir) => Ok(ViewerTheme::Noir),
        Ok(v1::ViewerTheme::Graphite) => Ok(ViewerTheme::Graphite),
        Ok(v1::ViewerTheme::Unspecified) | Err(_) => Err(ViewerClientError::Internal),
    }
}

const fn encode_viewer_theme(theme: ViewerTheme) -> v1::ViewerTheme {
    match theme {
        ViewerTheme::Dark => v1::ViewerTheme::Dark,
        ViewerTheme::Light => v1::ViewerTheme::Light,
        ViewerTheme::Hearth => v1::ViewerTheme::Hearth,
        ViewerTheme::Mirage => v1::ViewerTheme::Mirage,
        ViewerTheme::Glacier => v1::ViewerTheme::Glacier,
        ViewerTheme::Noir => v1::ViewerTheme::Noir,
        ViewerTheme::Graphite => v1::ViewerTheme::Graphite,
    }
}

fn decode_viewer_commit_summary(
    commit: v1::ViewerCommitSummary,
) -> Result<ViewerCommitSummary, ViewerClientError> {
    Ok(ViewerCommitSummary {
        id: CommitId::try_from(commit.id).map_err(|_| ViewerClientError::Internal)?,
        subject: commit.subject,
        body: commit.body,
        committed_at: MachineTimestamp::try_from(commit.committed_at.as_str())
            .map_err(|_| ViewerClientError::Internal)?,
        is_merge: commit.is_merge,
    })
}

fn encode_viewer_history_position(
    render_id: RenderHistoryId,
    page: HistoryPageNumber,
) -> Result<v1::ViewerHistoryPosition, ViewerClientError> {
    Ok(v1::ViewerHistoryPosition {
        render_id: encode_render_history_id(render_id)?,
        page: page.into(),
    })
}

fn decode_viewer_history_entry(
    entry: v1::ViewerHistoryEntry,
) -> Result<ViewerHistoryEntry, ViewerClientError> {
    let raw_id = i64::try_from(entry.id).map_err(|_| ViewerClientError::Internal)?;
    Ok(ViewerHistoryEntry {
        id: RenderHistoryId::try_new(raw_id).map_err(|_| ViewerClientError::Internal)?,
        title: entry.title,
        repository_name: ProjectName::try_new(entry.repository_name)
            .map_err(|_| ViewerClientError::Internal)?,
        kind: match v1::ViewerRecipeKind::try_from(entry.kind) {
            Ok(v1::ViewerRecipeKind::Diff) => ViewerRecipeKind::Diff,
            Ok(v1::ViewerRecipeKind::MergeDiff) => ViewerRecipeKind::MergeDiff,
            Ok(v1::ViewerRecipeKind::Unspecified) | Err(_) => {
                return Err(ViewerClientError::Internal);
            }
        },
        range_label: entry.range_label,
        rendered_at: MachineTimestamp::try_from(entry.rendered_at.as_str())
            .map_err(|_| ViewerClientError::Internal)?,
    })
}

fn encode_render_history_id(render_id: RenderHistoryId) -> Result<u64, ViewerClientError> {
    u64::try_from(i64::from(render_id)).map_err(|_| ViewerClientError::InvalidRequest)
}

fn decode_viewer_row_event(
    event: v1::stream_viewer_rows_response::Event,
) -> Result<ViewerRowEvent, ViewerClientError> {
    use v1::stream_viewer_rows_response::Event;

    match event {
        Event::FileStarted(event) => Ok(ViewerRowEvent::FileStarted {
            file: decode_viewer_diff_file_id(event.file_id)?,
        }),
        Event::UnifiedRows(event) => Ok(ViewerRowEvent::UnifiedRows {
            file: decode_viewer_diff_file_id(event.file_id)?,
            rows: event
                .rows
                .into_iter()
                .map(decode_viewer_unified_row)
                .collect::<Result<Vec<_>, _>>()?,
        }),
        Event::SplitRows(event) => Ok(ViewerRowEvent::SplitRows {
            file: decode_viewer_diff_file_id(event.file_id)?,
            rows: event
                .rows
                .into_iter()
                .map(decode_viewer_split_row)
                .collect::<Result<Vec<_>, _>>()?,
        }),
        Event::FileFinished(event) => Ok(ViewerRowEvent::FileFinished {
            file: decode_viewer_diff_file_id(event.file_id)?,
            line_number_digits: event.line_number_digits,
        }),
        Event::FileFailed(event) => Ok(ViewerRowEvent::FileFailed {
            file: decode_viewer_diff_file_id(event.file_id)?,
            code: match v1::ViewerFileFailureCode::try_from(event.code) {
                Ok(v1::ViewerFileFailureCode::SourceUnavailable) => {
                    ViewerFileFailureCode::SourceUnavailable
                }
                Ok(v1::ViewerFileFailureCode::ParseFailed) => ViewerFileFailureCode::ParseFailed,
                Ok(v1::ViewerFileFailureCode::RowTooLarge) => ViewerFileFailureCode::RowTooLarge,
                Ok(v1::ViewerFileFailureCode::Unspecified) | Err(_) => {
                    return Err(ViewerClientError::Internal);
                }
            },
            message: event.message,
            retryable: event.retryable,
        }),
    }
}

fn decode_viewer_diff_file_id(
    file_id: String,
) -> Result<gtl_wire::viewer::ViewerDiffFileId, ViewerClientError> {
    file_id.try_into().map_err(|_| ViewerClientError::Internal)
}

fn decode_viewer_unified_row(
    row: v1::ViewerUnifiedRow,
) -> Result<ViewerUnifiedRow, ViewerClientError> {
    use v1::viewer_unified_row::Row;

    match required(row.row)? {
        Row::Meta(text) => Ok(ViewerUnifiedRow::Meta(text)),
        Row::Hunk(text) => Ok(ViewerUnifiedRow::Hunk(text)),
        Row::Context(row) => Ok(ViewerUnifiedRow::Context(decode_viewer_unified_source_row(
            row,
        )?)),
        Row::Added(row) => Ok(ViewerUnifiedRow::Added(decode_viewer_unified_source_row(
            row,
        )?)),
        Row::Removed(row) => Ok(ViewerUnifiedRow::Removed(decode_viewer_unified_source_row(
            row,
        )?)),
    }
}

fn decode_viewer_unified_source_row(
    row: v1::ViewerUnifiedSourceRow,
) -> Result<ViewerUnifiedSourceRow, ViewerClientError> {
    Ok(ViewerUnifiedSourceRow {
        old_line_number: row.old_line_number,
        new_line_number: row.new_line_number,
        code: decode_viewer_code_line(required(row.code)?)?,
    })
}

fn decode_viewer_split_row(row: v1::ViewerSplitRow) -> Result<ViewerSplitRow, ViewerClientError> {
    use v1::viewer_split_row::Row;

    match required(row.row)? {
        Row::Meta(text) => Ok(ViewerSplitRow::Meta(text)),
        Row::Hunk(text) => Ok(ViewerSplitRow::Hunk(text)),
        Row::Context(row) => Ok(ViewerSplitRow::Context {
            old_line_number: row.old_line_number,
            new_line_number: row.new_line_number,
            code: decode_viewer_code_line(required(row.code)?)?,
        }),
        Row::Pair(row) => Ok(ViewerSplitRow::Pair {
            old: row.old.map(decode_viewer_split_cell).transpose()?,
            new: row.new.map(decode_viewer_split_cell).transpose()?,
        }),
    }
}

fn decode_viewer_split_cell(
    cell: v1::ViewerSplitCell,
) -> Result<ViewerSplitCell, ViewerClientError> {
    Ok(ViewerSplitCell {
        line_number: cell.line_number,
        code: decode_viewer_code_line(required(cell.code)?)?,
    })
}

fn decode_viewer_code_line(line: v1::ViewerCodeLine) -> Result<ViewerCodeLine, ViewerClientError> {
    let mut next_byte = 0_usize;
    let mut spans = Vec::with_capacity(line.spans.len().max(1));
    for span in line.spans {
        let byte_start =
            usize::try_from(span.byte_start).map_err(|_| ViewerClientError::Internal)?;
        let byte_end = usize::try_from(span.byte_end).map_err(|_| ViewerClientError::Internal)?;
        if byte_start != next_byte || byte_end < byte_start {
            return Err(ViewerClientError::Internal);
        }
        let text = line
            .text
            .get(byte_start..byte_end)
            .ok_or(ViewerClientError::Internal)?
            .to_owned();
        spans.push(ViewerCodeSpan {
            text,
            syntax_class: decode_viewer_syntax_class(span.syntax_class)?,
            changed: span.changed,
        });
        next_byte = byte_end;
    }
    if next_byte != line.text.len() {
        if !spans.is_empty() {
            return Err(ViewerClientError::Internal);
        }
        spans.push(ViewerCodeSpan {
            text: line.text.clone(),
            syntax_class: None,
            changed: false,
        });
    }
    Ok(ViewerCodeLine {
        text: line.text,
        spans,
        long_line_character_count: line
            .long_line_character_count
            .map(usize::try_from)
            .transpose()
            .map_err(|_| ViewerClientError::Internal)?,
    })
}

fn decode_viewer_syntax_class(
    syntax_class: i32,
) -> Result<Option<ViewerSyntaxClass>, ViewerClientError> {
    match v1::ViewerSyntaxClass::try_from(syntax_class) {
        Ok(v1::ViewerSyntaxClass::Unspecified) => Ok(None),
        Ok(v1::ViewerSyntaxClass::Keyword) => Ok(Some(ViewerSyntaxClass::Keyword)),
        Ok(v1::ViewerSyntaxClass::String) => Ok(Some(ViewerSyntaxClass::String)),
        Ok(v1::ViewerSyntaxClass::Comment) => Ok(Some(ViewerSyntaxClass::Comment)),
        Ok(v1::ViewerSyntaxClass::Type) => Ok(Some(ViewerSyntaxClass::Type)),
        Ok(v1::ViewerSyntaxClass::Function) => Ok(Some(ViewerSyntaxClass::Function)),
        Ok(v1::ViewerSyntaxClass::Number) => Ok(Some(ViewerSyntaxClass::Number)),
        Ok(v1::ViewerSyntaxClass::Constant) => Ok(Some(ViewerSyntaxClass::Constant)),
        Ok(v1::ViewerSyntaxClass::Operator) => Ok(Some(ViewerSyntaxClass::Operator)),
        Ok(v1::ViewerSyntaxClass::Tag) => Ok(Some(ViewerSyntaxClass::Tag)),
        Ok(v1::ViewerSyntaxClass::Variable) => Ok(Some(ViewerSyntaxClass::Variable)),
        Err(_) => Err(ViewerClientError::Internal),
    }
}

fn required<T>(value: Option<T>) -> Result<T, ViewerClientError> {
    value.ok_or(ViewerClientError::Internal)
}
