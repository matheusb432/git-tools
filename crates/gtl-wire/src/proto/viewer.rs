pub mod projects;
pub mod push;
mod recipe_label;
pub mod text;

use std::path::PathBuf;

use gtl_models::{
    diffs::{CommitId, CommitTimeRange, DiffLineCount, DiffViewTitle},
    failure::Failure,
    git::{GitHead, GitRevision},
    paths::{AbsoluteFilePath, ProjectName, RepositoryRelativePath},
    timestamps::MachineTimestamp,
    viewer::{
        HistoryPage, HistoryPageCount, HistoryPageNumber, HistoryPagePosition, HistoryRenderCount,
        RenderHistoryId, ViewerKeybinding, ViewerKeybindingAction, ViewerKeybindingPlatform,
        ViewerKeybindings, ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId,
        ViewerTabPlacement, ViewerVersion,
    },
};

use crate::{
    proto::failure::{decode_failure, encode_failure},
    v1,
    viewer::{
        EditSettingsRequest, FieldUpdate, FindViewerDiff, GetViewerHistoryCopy, ListViewerCommits,
        ListViewerHistory, MoveViewerTab, OpenViewerDiffFile, OpenViewerHistory, SearchViewerFiles,
        SelectViewerCommit, SetViewerPreference, StreamViewerRows, ViewerActiveState,
        ViewerActiveView, ViewerAppliedExtensionFilter, ViewerCodeLine, ViewerCodeSpan,
        ViewerCommandLine, ViewerCommitCursor, ViewerCommitPage, ViewerCommitSelection,
        ViewerCommitSummary, ViewerDiffDensity, ViewerDiffLayout, ViewerDiffSearchDirection,
        ViewerDiffSearchMatch, ViewerDiffSearchResult, ViewerFeedback, ViewerFileSearchResult,
        ViewerFileStatus, ViewerFileSummary, ViewerFooter, ViewerHistoryCopyPayload,
        ViewerHistoryCursor, ViewerHistoryEntry, ViewerHistoryPage, ViewerPreferences,
        ViewerRecipeKind, ViewerRenderOptions, ViewerRowEvent, ViewerRowStreamItem, ViewerShell,
        ViewerSplitCell, ViewerSplitRow, ViewerStateChanged, ViewerSyntaxClass, ViewerTab,
        ViewerTabRequest, ViewerTabState, ViewerTheme, ViewerUnifiedRow, ViewerUnifiedSourceRow,
        ViewerUserSettings, ViewerViewIdentity,
    },
};

/// Classifies a viewer value that cannot cross the protobuf boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ViewerCodecError {
    /// A valid contract value cannot be represented by the protobuf schema.
    #[error("viewer contract value cannot be represented by protobuf")]
    Unrepresentable,
    /// A protobuf message violates the viewer contract.
    #[error("protobuf message violates the viewer contract")]
    InvalidMessage,
    /// A named request field violates the viewer contract.
    #[error("request field `{field}` violates the viewer contract")]
    InvalidField { field: &'static str },
}

impl ViewerCodecError {
    /// Attributes an invalid message to the request field that carried it.
    #[must_use]
    pub const fn in_field(self, field: &'static str) -> Self {
        match self {
            Self::InvalidMessage => Self::InvalidField { field },
            other => other,
        }
    }

    /// The request field that violates the contract, when the decoder knows it.
    #[must_use]
    pub const fn field(self) -> Option<&'static str> {
        match self {
            Self::InvalidField { field } => Some(field),
            Self::Unrepresentable | Self::InvalidMessage => None,
        }
    }
}

/// Attributes a decoding failure to the request field `name`.
fn field(name: &'static str) -> impl Fn(ViewerCodecError) -> ViewerCodecError {
    move |error| error.in_field(name)
}

macro_rules! viewer_tab_codecs {
    ($($encode:ident => $request:ident; $decode:ident <= $response:ident;)+) => {
        $(
            pub fn $encode(request: ViewerTabRequest) -> v1::$request {
                v1::$request {
                    tab_id: request.tab_id.into(),
                }
            }

            pub fn $decode(
                response: v1::$response,
            ) -> Result<ViewerShell, ViewerCodecError> {
                decode_viewer_shell(required(response.shell)?)
            }
        )+
    };
}

macro_rules! viewer_shell_response_decoders {
    ($($decode:ident <= $response:ident;)+) => {
        $(
            pub fn $decode(
                response: v1::$response,
            ) -> Result<ViewerShell, ViewerCodecError> {
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
    encode_update_viewer_tab_request => UpdateViewerTabRequest;
    decode_update_viewer_tab_response <= UpdateViewerTabResponse;
    encode_clear_viewer_commit_selection_request => ClearViewerCommitSelectionRequest;
    decode_clear_viewer_commit_selection_response <= ClearViewerCommitSelectionResponse;
}

viewer_shell_response_decoders! {
    decode_get_viewer_shell_response <= GetViewerShellResponse;
    decode_move_viewer_tab_response <= MoveViewerTabResponse;
    decode_select_viewer_commit_response <= SelectViewerCommitResponse;
    decode_set_viewer_preference_response <= SetViewerPreferenceResponse;
    decode_open_viewer_history_response <= OpenViewerHistoryResponse;
}

#[must_use]
pub fn encode_move_viewer_tab_request(request: MoveViewerTab) -> v1::MoveViewerTabRequest {
    v1::MoveViewerTabRequest {
        tab_id: request.tab_id.into(),
        target_tab_id: request.target_tab_id.into(),
        placement: encode_viewer_tab_placement(request.placement) as i32,
    }
}

pub fn decode_move_viewer_tab_request(
    request: v1::MoveViewerTabRequest,
) -> Result<MoveViewerTab, ViewerCodecError> {
    Ok(MoveViewerTab {
        tab_id: ViewerTabId::try_new(request.tab_id)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        target_tab_id: ViewerTabId::try_new(request.target_tab_id)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        placement: decode_viewer_tab_placement(request.placement)?,
    })
}

const fn encode_viewer_tab_placement(placement: ViewerTabPlacement) -> v1::ViewerTabPlacement {
    match placement {
        ViewerTabPlacement::Before => v1::ViewerTabPlacement::Before,
        ViewerTabPlacement::After => v1::ViewerTabPlacement::After,
    }
}

fn decode_viewer_tab_placement(placement: i32) -> Result<ViewerTabPlacement, ViewerCodecError> {
    match v1::ViewerTabPlacement::try_from(placement) {
        Ok(v1::ViewerTabPlacement::Before) => Ok(ViewerTabPlacement::Before),
        Ok(v1::ViewerTabPlacement::After) => Ok(ViewerTabPlacement::After),
        Ok(v1::ViewerTabPlacement::Unspecified) | Err(_) => Err(ViewerCodecError::InvalidMessage),
    }
}

/// Encodes the complete viewer shell for a service response.
pub fn encode_viewer_shell(shell: ViewerShell) -> Result<v1::ViewerShell, ViewerCodecError> {
    Ok(v1::ViewerShell {
        version: shell.version.value(),
        focus_request_version: shell.focus_request_version.map(ViewerVersion::value),
        tabs: shell.tabs.into_iter().map(encode_viewer_tab).collect(),
        active: Some(encode_viewer_active_state(shell.active)?),
        preferences: Some(v1::ViewerPreferences {
            copy_with_line_context: Some(shell.preferences.copy_with_line_context),
            diff_files_sort: encode_diff_files_sort(shell.preferences.diff_files_sort) as i32,
            language: encode_viewer_language(shell.preferences.language) as i32,
            date_format: encode_viewer_date_format(shell.preferences.date_format) as i32,
            accessibility: Some(encode_viewer_accessibility(shell.preferences.accessibility)),
            sidebars: Some(encode_sidebar_visibility(shell.preferences.sidebars)),
            theme: encode_viewer_theme(shell.preferences.theme) as i32,
            render_options: Some(encode_viewer_render_options(
                shell.preferences.render_options,
            )),
            keybindings: Some(encode_viewer_keybindings(shell.preferences.keybindings)),
        }),
        feedback: shell.feedback.map(encode_viewer_feedback),
    })
}

fn encode_viewer_tab(tab: ViewerTab) -> v1::ViewerTab {
    v1::ViewerTab {
        details: tab.details.map(|details| v1::ViewerTabDetails {
            repository: details.repository.to_string_lossy().into_owned(),
            comparison: Some(recipe_label::encode(details.comparison)),
            range: details.range.map(|range| v1::ViewerTabRange {
                base: range.base.to_string(),
                head: range.head.to_string(),
            }),
        }),
        pinned: tab.pinned,
        custom_name: tab.custom_name,
        id: u64::from(tab.id),
        recipe_label: Some(recipe_label::encode(tab.label)),
        live: tab.live,
        state: match tab.state {
            ViewerTabState::Pending => v1::ViewerTabState::Pending,
            ViewerTabState::Ready => v1::ViewerTabState::Ready,
            ViewerTabState::Broken => v1::ViewerTabState::Broken,
            ViewerTabState::Error => v1::ViewerTabState::Error,
        } as i32,
    }
}

fn encode_viewer_active_state(
    active: ViewerActiveState,
) -> Result<v1::ViewerActiveState, ViewerCodecError> {
    let state = match active {
        ViewerActiveState::Empty => v1::viewer_active_state::State::Empty(v1::Empty {}),
        ViewerActiveState::Pending { tab_id } => {
            v1::viewer_active_state::State::Pending(v1::ViewerPendingState {
                tab_id: u64::from(tab_id),
            })
        }
        ViewerActiveState::Broken { tab_id, failure } => {
            v1::viewer_active_state::State::Broken(encode_viewer_failure(tab_id, &failure))
        }
        ViewerActiveState::Error { tab_id, failure } => {
            v1::viewer_active_state::State::Error(encode_viewer_failure(tab_id, &failure))
        }
        ViewerActiveState::Ready { view } => {
            v1::viewer_active_state::State::Ready(Box::new(v1::ViewerReadyState {
                view: Some(encode_viewer_active_view(*view)?),
            }))
        }
    };
    Ok(v1::ViewerActiveState { state: Some(state) })
}

fn encode_viewer_failure(tab_id: ViewerTabId, failure: &Failure) -> v1::ViewerFailureState {
    v1::ViewerFailureState {
        tab_id: u64::from(tab_id),
        failure: Some(encode_failure(failure)),
    }
}

fn encode_viewer_active_view(
    view: ViewerActiveView,
) -> Result<v1::ViewerActiveView, ViewerCodecError> {
    Ok(v1::ViewerActiveView {
        modified_files: view.modified_files,
        identity: Some(encode_viewer_view_identity(view.identity)),
        content_id: Some(view.content_id.into_digest().to_vec()),
        row_source: match view.row_source {
            crate::viewer::ViewerRowSourceState::Ready => v1::ViewerRowSourceState::Ready,
            crate::viewer::ViewerRowSourceState::Pending => v1::ViewerRowSourceState::Pending,
            crate::viewer::ViewerRowSourceState::Failed => v1::ViewerRowSourceState::Failed,
        } as i32,
        view_title: Some(encode_diff_view_title(view.title)),
        repository_name: view.repository_name.to_string(),
        branch: view.branch.to_string(),
        upstream: view.upstream.to_string(),
        command: Some(v1::ViewerCommandLine {
            lead: view.command.lead,
            range: view.command.range,
            trail: view.command.trail,
        }),
        files: view
            .files
            .into_iter()
            .map(|file| {
                Ok(v1::ViewerFileSummary {
                    source_id: file.source_id.map(|value| value.into_digest().to_vec()),
                    id: file.id.as_str().to_owned(),
                    path: file.path.to_string_lossy().into_owned(),
                    absolute_path: file.absolute_path.as_path().to_string_lossy().into_owned(),
                    anchor_id: file.anchor_id,
                    added: u32::try_from(file.added.value())
                        .map_err(|_| ViewerCodecError::Unrepresentable)?,
                    removed: u32::try_from(file.removed.value())
                        .map_err(|_| ViewerCodecError::Unrepresentable)?,
                    status: match file.status {
                        ViewerFileStatus::Added => v1::ViewerFileStatus::Added,
                        ViewerFileStatus::Deleted => v1::ViewerFileStatus::Deleted,
                        ViewerFileStatus::Renamed => v1::ViewerFileStatus::Renamed,
                        ViewerFileStatus::Modified => v1::ViewerFileStatus::Modified,
                    } as i32,
                    can_open_in_editor: file.can_open_in_editor,
                    initially_expanded: file.initially_expanded,
                    row_count: u32::try_from(file.row_count)
                        .map_err(|_| ViewerCodecError::Unrepresentable)?,
                })
            })
            .collect::<Result<Vec<_>, ViewerCodecError>>()?,
        commit_count: u32::try_from(view.commit_count)
            .map_err(|_| ViewerCodecError::Unrepresentable)?,
        commit_selection: Some(encode_viewer_commit_selection(view.commit_selection)),
        footer: Some(v1::ViewerFooter {
            command: view.footer.command,
        }),
        extension_filter: view
            .extension_filter
            .map(|applied| v1::ViewerAppliedExtensionFilter {
                filter: Some(file_filters::encode_extension_filter(applied.filter)),
                hidden_paths: applied
                    .hidden_paths
                    .into_iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
            }),
        changes_since: view.changes_since.map(|timestamp| timestamp.to_string()),
    })
}

fn encode_diff_view_title(title: DiffViewTitle) -> v1::ViewerDiffViewTitle {
    use v1::viewer_diff_view_title::Title;

    let title = match title {
        DiffViewTitle::Diff => Title::Diff(v1::Empty {}),
        DiffViewTitle::MergeDiff => Title::MergeDiff(v1::Empty {}),
        DiffViewTitle::Commit { id } => Title::CommitId(id.to_string()),
        DiffViewTitle::Named { name } => Title::Name(name.to_string()),
    };
    v1::ViewerDiffViewTitle { title: Some(title) }
}

fn decode_diff_view_title(
    title: v1::ViewerDiffViewTitle,
) -> Result<DiffViewTitle, ViewerCodecError> {
    use v1::viewer_diff_view_title::Title;

    Ok(match required(title.title)? {
        Title::Diff(v1::Empty {}) => DiffViewTitle::Diff,
        Title::MergeDiff(v1::Empty {}) => DiffViewTitle::MergeDiff,
        Title::CommitId(id) => DiffViewTitle::Commit {
            id: CommitId::try_new(id).map_err(|_| ViewerCodecError::InvalidMessage)?,
        },
        Title::Name(name) => DiffViewTitle::Named {
            name: ProjectName::try_new(name).map_err(|_| ViewerCodecError::InvalidMessage)?,
        },
    })
}

fn encode_viewer_commit_selection(selection: ViewerCommitSelection) -> v1::ViewerCommitSelection {
    let (state, commit_id, failure) = match selection {
        ViewerCommitSelection::None => (v1::ViewerCommitSelectionState::None, None, None),
        ViewerCommitSelection::Pending { id } => (
            v1::ViewerCommitSelectionState::Pending,
            Some(id.to_string()),
            None,
        ),
        ViewerCommitSelection::Ready { id } => (
            v1::ViewerCommitSelectionState::Ready,
            Some(id.to_string()),
            None,
        ),
        ViewerCommitSelection::Error { id, failure } => (
            v1::ViewerCommitSelectionState::Error,
            Some(id.to_string()),
            Some(encode_failure(&failure)),
        ),
    };
    v1::ViewerCommitSelection {
        state: state as i32,
        commit_id,
        failure,
    }
}

fn encode_viewer_feedback(feedback: ViewerFeedback) -> v1::ViewerFeedback {
    let (kind, labels) = match feedback {
        ViewerFeedback::SnapshotRecipesSkipped { labels } => {
            (v1::ViewerFeedbackKind::SnapshotRecipesSkipped, labels)
        }
    };
    v1::ViewerFeedback {
        kind: kind as i32,
        recipe_labels: labels.into_iter().map(recipe_label::encode).collect(),
    }
}

#[must_use]
pub fn encode_select_viewer_commit_request(
    request: SelectViewerCommit,
) -> v1::SelectViewerCommitRequest {
    let SelectViewerCommit { tab_id, id } = request;
    v1::SelectViewerCommitRequest {
        tab_id: tab_id.into(),
        commit_id: id.to_string(),
    }
}

#[must_use]
pub fn encode_set_viewer_preference_request(
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

/// Decodes a preference mutation at the service boundary.
pub fn decode_set_viewer_preference_request(
    request: v1::SetViewerPreferenceRequest,
) -> Result<SetViewerPreference, ViewerCodecError> {
    use v1::set_viewer_preference_request::Preference;

    match required(request.preference)? {
        Preference::Layout(layout) => Ok(SetViewerPreference::Layout(decode_viewer_diff_layout(
            layout,
        )?)),
        Preference::Density(density) => Ok(SetViewerPreference::Density(
            decode_viewer_diff_density(density)?,
        )),
        Preference::Theme(theme) => Ok(SetViewerPreference::Theme(decode_viewer_theme(theme)?)),
    }
}

pub fn encode_list_viewer_commits_request(
    request: ListViewerCommits,
) -> v1::ListViewerCommitsRequest {
    v1::ListViewerCommitsRequest {
        identity: Some(encode_viewer_view_identity(request.identity)),
        cursor: request.cursor.map(ViewerCommitCursor::into_inner),
    }
}

pub fn decode_list_viewer_commits_response(
    response: v1::ListViewerCommitsResponse,
) -> Result<ViewerCommitPage, ViewerCodecError> {
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

pub fn encode_list_viewer_history_request(
    request: ListViewerHistory,
) -> Result<v1::ListViewerHistoryRequest, ViewerCodecError> {
    use v1::list_viewer_history_request::{Cursor, Filter};

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
    let filter = match request.filter {
        crate::viewer::ViewerHistoryFilter::All => Filter::AllProjects(v1::Empty {}),
        crate::viewer::ViewerHistoryFilter::Unassociated => Filter::Unassociated(v1::Empty {}),
        crate::viewer::ViewerHistoryFilter::Project { name } => {
            Filter::ProjectName(name.to_string())
        }
    };
    Ok(v1::ListViewerHistoryRequest {
        filter: Some(filter),
        cursor: Some(cursor),
    })
}

/// Decodes a history-page request at the service boundary.
pub fn decode_list_viewer_history_request(
    request: v1::ListViewerHistoryRequest,
) -> Result<ListViewerHistory, ViewerCodecError> {
    use v1::list_viewer_history_request::{Cursor, Filter};

    let cursor = match required(request.cursor)? {
        Cursor::Newest(_) => ViewerHistoryCursor::Newest,
        Cursor::OlderThan(position) => ViewerHistoryCursor::OlderThan {
            render_id: decode_render_history_id(position.render_id)?,
            page: HistoryPageNumber::try_new(position.page)
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
        },
        Cursor::NewerThan(position) => ViewerHistoryCursor::NewerThan {
            render_id: decode_render_history_id(position.render_id)?,
            page: HistoryPageNumber::try_new(position.page)
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
        },
        Cursor::Oldest(_) => ViewerHistoryCursor::Oldest,
    };
    let filter = match required(request.filter)? {
        Filter::AllProjects(_) => crate::viewer::ViewerHistoryFilter::All,
        Filter::Unassociated(_) => crate::viewer::ViewerHistoryFilter::Unassociated,
        Filter::ProjectName(name) => crate::viewer::ViewerHistoryFilter::Project {
            name: name
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
        },
    };
    Ok(ListViewerHistory { cursor, filter })
}

pub fn decode_list_viewer_history_response(
    response: v1::ListViewerHistoryResponse,
) -> Result<ViewerHistoryPage, ViewerCodecError> {
    let entries = response
        .entries
        .into_iter()
        .map(decode_viewer_history_entry)
        .collect::<Result<Vec<_>, _>>()?;
    let position = match response.position {
        Some(position) => HistoryPagePosition::Page(
            HistoryPage::new(
                HistoryPageNumber::try_new(position.number)
                    .map_err(|_| ViewerCodecError::InvalidMessage)?,
                HistoryPageCount::try_new(position.count)
                    .map_err(|_| ViewerCodecError::InvalidMessage)?,
            )
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        ),
        None if entries.is_empty() => HistoryPagePosition::Empty,
        None => return Err(ViewerCodecError::InvalidMessage),
    };
    if response.projects.len() > usize::from(gtl_models::projects::catalogue::PROJECTS_MAX) {
        return Err(ViewerCodecError::InvalidMessage);
    }
    Ok(ViewerHistoryPage {
        projects: response
            .projects
            .into_iter()
            .map(|name| {
                name.try_into()
                    .map_err(|_| ViewerCodecError::InvalidMessage)
            })
            .collect::<Result<_, _>>()?,
        entries,
        total_count: HistoryRenderCount::new(response.total_count),
        position,
        has_newer: response.has_newer,
        has_older: response.has_older,
    })
}

/// Encodes a history page for a service response.
pub fn encode_list_viewer_history_response(
    page: ViewerHistoryPage,
) -> Result<v1::ListViewerHistoryResponse, ViewerCodecError> {
    Ok(v1::ListViewerHistoryResponse {
        projects: page
            .projects
            .into_iter()
            .map(|name| name.to_string())
            .collect(),
        entries: page
            .entries
            .into_iter()
            .map(|entry| {
                Ok(v1::ViewerHistoryEntry {
                    id: encode_render_history_id(entry.id)?,
                    recipe_label: Some(recipe_label::encode(entry.label)),
                    repository_name: entry.repository_name.to_string(),
                    kind: match entry.kind {
                        ViewerRecipeKind::Diff => v1::ViewerRecipeKind::Diff,
                        ViewerRecipeKind::MergeDiff => v1::ViewerRecipeKind::MergeDiff,
                    } as i32,
                    range_label: entry.range_label,
                    rendered_at: entry.rendered_at.as_ref().to_owned(),
                })
            })
            .collect::<Result<Vec<_>, ViewerCodecError>>()?,
        total_count: page.total_count.into_inner(),
        position: page
            .position
            .page()
            .map(|position| v1::ViewerHistoryPagePosition {
                number: u32::from(position.number()),
                count: u32::from(position.count()),
            }),
        has_newer: page.has_newer,
        has_older: page.has_older,
    })
}

pub fn encode_open_viewer_history_request(
    request: OpenViewerHistory,
) -> Result<v1::OpenViewerHistoryRequest, ViewerCodecError> {
    Ok(v1::OpenViewerHistoryRequest {
        render_id: encode_render_history_id(request.render_id)?,
    })
}

pub fn encode_get_viewer_history_copy_request(
    request: GetViewerHistoryCopy,
) -> Result<v1::GetViewerHistoryCopyRequest, ViewerCodecError> {
    Ok(v1::GetViewerHistoryCopyRequest {
        render_id: encode_render_history_id(request.render_id)?,
    })
}

#[must_use]
pub fn decode_get_viewer_history_copy_response(
    response: v1::GetViewerHistoryCopyResponse,
) -> ViewerHistoryCopyPayload {
    ViewerHistoryCopyPayload {
        json: response.json,
    }
}

const fn encode_viewer_language(
    language: gtl_models::settings::ViewerLanguage,
) -> v1::ViewerLanguage {
    match language {
        gtl_models::settings::ViewerLanguage::EnUs => v1::ViewerLanguage::EnUs,
        gtl_models::settings::ViewerLanguage::PtBr => v1::ViewerLanguage::PtBr,
    }
}

fn decode_viewer_language(
    language: i32,
) -> Result<gtl_models::settings::ViewerLanguage, ViewerCodecError> {
    match v1::ViewerLanguage::try_from(language) {
        Ok(v1::ViewerLanguage::EnUs) => Ok(gtl_models::settings::ViewerLanguage::EnUs),
        Ok(v1::ViewerLanguage::PtBr) => Ok(gtl_models::settings::ViewerLanguage::PtBr),
        Ok(v1::ViewerLanguage::Unspecified) | Err(_) => Err(ViewerCodecError::InvalidMessage),
    }
}

fn encode_viewer_language_update(
    update: &FieldUpdate<gtl_models::settings::ViewerLanguage>,
) -> Option<v1::ViewerLanguageFieldUpdate> {
    use v1::viewer_language_field_update::Operation;
    let operation = match update {
        FieldUpdate::Unchanged => return None,
        FieldUpdate::Clear => Operation::Clear(v1::ClearSetting {}),
        FieldUpdate::Update(language) => {
            Operation::Update(encode_viewer_language(*language) as i32)
        }
    };
    Some(v1::ViewerLanguageFieldUpdate {
        operation: Some(operation),
    })
}

fn decode_viewer_language_update(
    update: Option<v1::ViewerLanguageFieldUpdate>,
) -> Result<FieldUpdate<gtl_models::settings::ViewerLanguage>, ViewerCodecError> {
    use v1::viewer_language_field_update::Operation;
    Ok(match update {
        None => FieldUpdate::Unchanged,
        Some(update) => match required(update.operation)? {
            Operation::Clear(_) => FieldUpdate::Clear,
            Operation::Update(language) => FieldUpdate::Update(decode_viewer_language(language)?),
        },
    })
}

const fn encode_viewer_date_format(
    date_format: gtl_models::settings::ViewerDateFormat,
) -> v1::ViewerDateFormat {
    use gtl_models::settings::ViewerDateFormat;
    match date_format {
        ViewerDateFormat::Iso => v1::ViewerDateFormat::Iso,
        ViewerDateFormat::DayFirst => v1::ViewerDateFormat::DayFirst,
        ViewerDateFormat::MonthFirst => v1::ViewerDateFormat::MonthFirst,
        ViewerDateFormat::Relative => v1::ViewerDateFormat::Relative,
    }
}

fn decode_viewer_date_format(
    date_format: i32,
) -> Result<gtl_models::settings::ViewerDateFormat, ViewerCodecError> {
    use gtl_models::settings::ViewerDateFormat;
    match v1::ViewerDateFormat::try_from(date_format) {
        Ok(v1::ViewerDateFormat::Iso) => Ok(ViewerDateFormat::Iso),
        Ok(v1::ViewerDateFormat::DayFirst) => Ok(ViewerDateFormat::DayFirst),
        Ok(v1::ViewerDateFormat::MonthFirst) => Ok(ViewerDateFormat::MonthFirst),
        Ok(v1::ViewerDateFormat::Relative) => Ok(ViewerDateFormat::Relative),
        Ok(v1::ViewerDateFormat::Unspecified) | Err(_) => Err(ViewerCodecError::InvalidMessage),
    }
}

fn encode_viewer_date_format_update(
    update: &FieldUpdate<gtl_models::settings::ViewerDateFormat>,
) -> Option<v1::ViewerDateFormatFieldUpdate> {
    use v1::viewer_date_format_field_update::Operation;
    let operation = match update {
        FieldUpdate::Unchanged => return None,
        FieldUpdate::Clear => Operation::Clear(v1::ClearSetting {}),
        FieldUpdate::Update(date_format) => {
            Operation::Update(encode_viewer_date_format(*date_format) as i32)
        }
    };
    Some(v1::ViewerDateFormatFieldUpdate {
        operation: Some(operation),
    })
}

fn decode_viewer_date_format_update(
    update: Option<v1::ViewerDateFormatFieldUpdate>,
) -> Result<FieldUpdate<gtl_models::settings::ViewerDateFormat>, ViewerCodecError> {
    use v1::viewer_date_format_field_update::Operation;
    Ok(match update {
        None => FieldUpdate::Unchanged,
        Some(update) => match required(update.operation)? {
            Operation::Clear(_) => FieldUpdate::Clear,
            Operation::Update(date_format) => {
                FieldUpdate::Update(decode_viewer_date_format(date_format)?)
            }
        },
    })
}

fn encode_viewer_accessibility(
    value: gtl_models::settings::ViewerAccessibility,
) -> v1::ViewerAccessibility {
    v1::ViewerAccessibility {
        ui_scale_percent: value.ui_scale_percent.into_inner(),
        reduce_motion: value.reduce_motion,
    }
}

fn decode_viewer_accessibility(
    value: v1::ViewerAccessibility,
) -> Result<gtl_models::settings::ViewerAccessibility, ViewerCodecError> {
    Ok(gtl_models::settings::ViewerAccessibility {
        ui_scale_percent: gtl_models::settings::ViewerScalePercent::try_new(value.ui_scale_percent)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        reduce_motion: value.reduce_motion,
    })
}

fn encode_viewer_scale_update(
    update: &FieldUpdate<gtl_models::settings::ViewerScalePercent>,
) -> Option<v1::ViewerScaleFieldUpdate> {
    use v1::viewer_scale_field_update::Operation;
    let operation = match update {
        FieldUpdate::Unchanged => return None,
        FieldUpdate::Clear => Operation::Clear(v1::ClearSetting {}),
        FieldUpdate::Update(value) => Operation::Update(value.into_inner()),
    };
    Some(v1::ViewerScaleFieldUpdate {
        operation: Some(operation),
    })
}

fn decode_viewer_scale_update(
    update: Option<v1::ViewerScaleFieldUpdate>,
) -> Result<FieldUpdate<gtl_models::settings::ViewerScalePercent>, ViewerCodecError> {
    use v1::viewer_scale_field_update::Operation;
    Ok(match update {
        None => FieldUpdate::Unchanged,
        Some(update) => match required(update.operation)? {
            Operation::Clear(_) => FieldUpdate::Clear,
            Operation::Update(value) => FieldUpdate::Update(
                gtl_models::settings::ViewerScalePercent::try_new(value)
                    .map_err(|_| ViewerCodecError::InvalidMessage)?,
            ),
        },
    })
}

pub fn decode_get_viewer_settings_response(
    response: v1::GetViewerSettingsResponse,
) -> Result<ViewerUserSettings, ViewerCodecError> {
    Ok(ViewerUserSettings {
        language: decode_viewer_language(response.language)?,
        date_format: decode_viewer_date_format(response.date_format)?,
        accessibility: decode_viewer_accessibility(required(response.accessibility)?)?,
        revision: response
            .revision
            .parse()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        focus_window_on_diff: required(response.focus_window_on_diff)?,
        copy_with_line_context: required(response.copy_with_line_context)?,
        diff_files_sort: decode_diff_files_sort(response.diff_files_sort)?,
        sidebars: decode_sidebar_visibility(required(response.sidebars)?),
        projects_sort: decode_projects_sort(response.projects_sort)?,
        projects_page_size: gtl_models::settings::ProjectsPageSize::try_new(
            response.projects_page_size,
        )
        .map_err(|_| ViewerCodecError::InvalidMessage)?,
        configuration_path: response.configuration_path,
        configured_theme: response
            .configured_theme
            .map(decode_viewer_theme)
            .transpose()?,
        effective_theme: decode_viewer_theme(response.effective_theme)?,
        render_options: decode_viewer_render_options(required(response.render_options)?)?,
        push_confirmation_required: response.push_confirmation_required,
        viewer_push_no_confirmation_projects: decode_project_names(
            response.viewer_push_no_confirmation_projects,
        )?,
    })
}

/// Encodes the complete viewer settings snapshot for a service response.
#[must_use]
pub fn encode_get_viewer_settings_response(
    settings: ViewerUserSettings,
) -> v1::GetViewerSettingsResponse {
    v1::GetViewerSettingsResponse {
        language: encode_viewer_language(settings.language) as i32,
        date_format: encode_viewer_date_format(settings.date_format) as i32,
        accessibility: Some(encode_viewer_accessibility(settings.accessibility)),
        revision: settings.revision.to_string(),
        focus_window_on_diff: Some(settings.focus_window_on_diff),
        copy_with_line_context: Some(settings.copy_with_line_context),
        diff_files_sort: encode_diff_files_sort(settings.diff_files_sort) as i32,
        sidebars: Some(encode_sidebar_visibility(settings.sidebars)),
        projects_sort: encode_projects_sort(settings.projects_sort) as i32,
        projects_page_size: settings.projects_page_size.into_inner(),
        configuration_path: settings.configuration_path,
        configured_theme: settings
            .configured_theme
            .map(|theme| encode_viewer_theme(theme) as i32),
        effective_theme: encode_viewer_theme(settings.effective_theme) as i32,
        render_options: Some(encode_viewer_render_options(settings.render_options)),
        push_confirmation_required: settings.push_confirmation_required,
        viewer_push_no_confirmation_projects: settings
            .viewer_push_no_confirmation_projects
            .into_iter()
            .map(|name| name.to_string())
            .collect(),
    }
}

#[must_use]
pub fn encode_edit_settings_request(request: &EditSettingsRequest) -> v1::EditSettingsRequest {
    use v1::{density_field_update, layout_field_update, theme_field_update};
    let theme = match &request.theme {
        FieldUpdate::Unchanged => None,
        FieldUpdate::Clear => Some(v1::ThemeFieldUpdate {
            operation: Some(theme_field_update::Operation::Clear(v1::ClearSetting {})),
        }),
        FieldUpdate::Update(value) => Some(v1::ThemeFieldUpdate {
            operation: Some(theme_field_update::Operation::Update(
                encode_viewer_theme(*value) as i32,
            )),
        }),
    };
    let layout = match &request.layout {
        FieldUpdate::Unchanged => None,
        FieldUpdate::Clear => Some(v1::LayoutFieldUpdate {
            operation: Some(layout_field_update::Operation::Clear(v1::ClearSetting {})),
        }),
        FieldUpdate::Update(value) => Some(v1::LayoutFieldUpdate {
            operation: Some(layout_field_update::Operation::Update(
                encode_viewer_diff_layout(*value) as i32,
            )),
        }),
    };
    let density = match &request.density {
        FieldUpdate::Unchanged => None,
        FieldUpdate::Clear => Some(v1::DensityFieldUpdate {
            operation: Some(density_field_update::Operation::Clear(v1::ClearSetting {})),
        }),
        FieldUpdate::Update(value) => Some(v1::DensityFieldUpdate {
            operation: Some(density_field_update::Operation::Update(
                encode_viewer_diff_density(*value) as i32,
            )),
        }),
    };
    let wrap_lines = encode_bool_field_update(&request.wrap_lines);
    let push_confirmation_required = encode_bool_field_update(&request.push_confirmation_required);
    v1::EditSettingsRequest {
        ui_scale_percent: encode_viewer_scale_update(&request.ui_scale_percent),
        reduce_motion: encode_bool_field_update(&request.reduce_motion),
        language: encode_viewer_language_update(&request.language),
        date_format: encode_viewer_date_format_update(&request.date_format),
        expected_revision: request
            .expected_revision
            .map(|revision| revision.to_string()),
        focus_window_on_diff: encode_bool_field_update(&request.focus_window_on_diff),
        files_sidebar_visible: encode_bool_field_update(&request.files_sidebar_visible),
        commits_sidebar_visible: encode_bool_field_update(&request.commits_sidebar_visible),
        wrap_lines,
        copy_with_line_context: encode_bool_field_update(&request.copy_with_line_context),
        diff_files_sort: encode_diff_files_sort_update(&request.diff_files_sort),
        projects_sort: encode_projects_sort_update(&request.projects_sort),
        projects_page_size: encode_projects_page_size_update(&request.projects_page_size),
        theme,
        layout,
        density,
        push_confirmation_required,
        viewer_push_no_confirmation_projects: encode_project_names_update(
            request.viewer_push_no_confirmation_projects.clone(),
        ),
    }
}

fn encode_bool_field_update(update: &FieldUpdate<bool>) -> Option<v1::BoolFieldUpdate> {
    let operation = match update {
        FieldUpdate::Unchanged => return None,
        FieldUpdate::Clear => v1::bool_field_update::Operation::Clear(v1::ClearSetting {}),
        FieldUpdate::Update(value) => v1::bool_field_update::Operation::Update(*value),
    };
    Some(v1::BoolFieldUpdate {
        operation: Some(operation),
    })
}

pub fn decode_edit_settings_request(
    request: v1::EditSettingsRequest,
) -> Result<EditSettingsRequest, ViewerCodecError> {
    Ok(EditSettingsRequest {
        ui_scale_percent: decode_viewer_scale_update(request.ui_scale_percent)
            .map_err(field("ui_scale_percent"))?,
        reduce_motion: decode_bool_field_update(request.reduce_motion)
            .map_err(field("reduce_motion"))?,
        language: decode_viewer_language_update(request.language).map_err(field("language"))?,
        date_format: decode_viewer_date_format_update(request.date_format)
            .map_err(field("date_format"))?,
        expected_revision: decode_user_settings_revision(request.expected_revision)
            .map_err(field("expected_revision"))?,
        focus_window_on_diff: decode_bool_field_update(request.focus_window_on_diff)
            .map_err(field("focus_window_on_diff"))?,
        files_sidebar_visible: decode_bool_field_update(request.files_sidebar_visible)
            .map_err(field("files_sidebar_visible"))?,
        commits_sidebar_visible: decode_bool_field_update(request.commits_sidebar_visible)
            .map_err(field("commits_sidebar_visible"))?,
        projects_page_size: decode_projects_page_size_update(request.projects_page_size)
            .map_err(field("projects_page_size"))?,
        projects_sort: decode_projects_sort_update(request.projects_sort)
            .map_err(field("projects_sort"))?,
        diff_files_sort: decode_diff_files_sort_update(request.diff_files_sort)
            .map_err(field("diff_files_sort"))?,
        theme: decode_theme_update(request.theme).map_err(field("theme"))?,
        layout: decode_layout_update(request.layout).map_err(field("layout"))?,
        density: decode_density_update(request.density).map_err(field("density"))?,
        wrap_lines: decode_bool_field_update(request.wrap_lines).map_err(field("wrap_lines"))?,
        copy_with_line_context: decode_bool_field_update(request.copy_with_line_context)
            .map_err(field("copy_with_line_context"))?,
        viewer_push_no_confirmation_projects: decode_project_names_update(
            request.viewer_push_no_confirmation_projects,
        )
        .map_err(field("viewer_push_no_confirmation_projects"))?,
        push_confirmation_required: decode_bool_field_update(request.push_confirmation_required)
            .map_err(field("push_confirmation_required"))?,
    })
}

fn decode_projects_sort_update(
    update: Option<v1::ProjectsSortFieldUpdate>,
) -> Result<FieldUpdate<gtl_models::settings::ProjectsSort>, ViewerCodecError> {
    let Some(update) = update else {
        return Ok(FieldUpdate::Unchanged);
    };
    Ok(match required(update.operation)? {
        v1::projects_sort_field_update::Operation::Clear(_) => FieldUpdate::Clear,
        v1::projects_sort_field_update::Operation::Update(value) => {
            FieldUpdate::Update(decode_projects_sort(value)?)
        }
    })
}

fn decode_diff_files_sort_update(
    update: Option<v1::DiffFilesSortFieldUpdate>,
) -> Result<FieldUpdate<gtl_models::settings::DiffFilesSort>, ViewerCodecError> {
    let Some(update) = update else {
        return Ok(FieldUpdate::Unchanged);
    };
    Ok(match required(update.operation)? {
        v1::diff_files_sort_field_update::Operation::Clear(_) => FieldUpdate::Clear,
        v1::diff_files_sort_field_update::Operation::Update(value) => {
            FieldUpdate::Update(decode_diff_files_sort(value)?)
        }
    })
}

fn decode_theme_update(
    update: Option<v1::ThemeFieldUpdate>,
) -> Result<FieldUpdate<ViewerTheme>, ViewerCodecError> {
    let Some(update) = update else {
        return Ok(FieldUpdate::Unchanged);
    };
    Ok(match required(update.operation)? {
        v1::theme_field_update::Operation::Clear(_) => FieldUpdate::Clear,
        v1::theme_field_update::Operation::Update(value) => {
            FieldUpdate::Update(decode_viewer_theme(value)?)
        }
    })
}

fn decode_layout_update(
    update: Option<v1::LayoutFieldUpdate>,
) -> Result<FieldUpdate<ViewerDiffLayout>, ViewerCodecError> {
    let Some(update) = update else {
        return Ok(FieldUpdate::Unchanged);
    };
    Ok(match required(update.operation)? {
        v1::layout_field_update::Operation::Clear(_) => FieldUpdate::Clear,
        v1::layout_field_update::Operation::Update(value) => {
            FieldUpdate::Update(decode_viewer_diff_layout(value)?)
        }
    })
}

fn decode_density_update(
    update: Option<v1::DensityFieldUpdate>,
) -> Result<FieldUpdate<ViewerDiffDensity>, ViewerCodecError> {
    let Some(update) = update else {
        return Ok(FieldUpdate::Unchanged);
    };
    Ok(match required(update.operation)? {
        v1::density_field_update::Operation::Clear(_) => FieldUpdate::Clear,
        v1::density_field_update::Operation::Update(value) => {
            FieldUpdate::Update(decode_viewer_diff_density(value)?)
        }
    })
}

fn decode_user_settings_revision(
    revision: Option<String>,
) -> Result<Option<gtl_models::settings::UserSettingsRevision>, ViewerCodecError> {
    revision
        .map(|value| value.parse())
        .transpose()
        .map_err(|_| ViewerCodecError::InvalidMessage)
}

fn decode_projects_page_size_update(
    update: Option<v1::ProjectsPageSizeFieldUpdate>,
) -> Result<FieldUpdate<gtl_models::settings::ProjectsPageSize>, ViewerCodecError> {
    let Some(update) = update else {
        return Ok(FieldUpdate::Unchanged);
    };
    Ok(match required(update.operation)? {
        v1::projects_page_size_field_update::Operation::Clear(_) => FieldUpdate::Clear,
        v1::projects_page_size_field_update::Operation::Update(value) => FieldUpdate::Update(
            gtl_models::settings::ProjectsPageSize::try_new(value)
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
        ),
    })
}

#[must_use]
pub fn encode_open_viewer_diff_file_request(
    request: OpenViewerDiffFile,
) -> v1::OpenViewerDiffFileRequest {
    let OpenViewerDiffFile { identity, file } = request;
    v1::OpenViewerDiffFileRequest {
        identity: Some(encode_viewer_view_identity(identity)),
        file_id: file.as_str().to_owned(),
    }
}

pub fn decode_open_viewer_diff_file_request(
    request: v1::OpenViewerDiffFileRequest,
) -> Result<OpenViewerDiffFile, ViewerCodecError> {
    Ok(OpenViewerDiffFile {
        identity: decode_viewer_view_identity(required(request.identity)?)?,
        file: decode_viewer_diff_file_id(request.file_id)?,
    })
}

#[must_use]
pub fn encode_stream_viewer_rows_request(request: StreamViewerRows) -> v1::StreamViewerRowsRequest {
    v1::StreamViewerRowsRequest {
        identity: Some(encode_viewer_view_identity(request.identity)),
        file_id: request.file.map(|file| file.as_str().to_owned()),
        row_range: request.row_range.map(|range| v1::ViewerRowRange {
            start: range.start(),
            count: range.count(),
        }),
    }
}

pub fn decode_stream_viewer_rows_response(
    response: v1::StreamViewerRowsResponse,
) -> Result<ViewerRowStreamItem, ViewerCodecError> {
    Ok(ViewerRowStreamItem {
        identity: decode_viewer_view_identity(required(response.identity)?)?,
        sequence: response.sequence,
        event: decode_viewer_row_event(required(response.event)?)?,
    })
}

#[must_use]
pub fn encode_search_viewer_files_request(
    request: SearchViewerFiles,
) -> v1::SearchViewerFilesRequest {
    v1::SearchViewerFilesRequest {
        identity: Some(encode_viewer_view_identity(request.identity)),
        query: request.query,
    }
}

pub fn decode_search_viewer_files_request(
    request: v1::SearchViewerFilesRequest,
) -> Result<SearchViewerFiles, ViewerCodecError> {
    Ok(SearchViewerFiles {
        identity: decode_viewer_view_identity(required(request.identity)?)?,
        query: request.query,
    })
}

#[must_use]
pub fn encode_search_viewer_files_response(
    result: ViewerFileSearchResult,
) -> v1::SearchViewerFilesResponse {
    v1::SearchViewerFilesResponse {
        identity: Some(encode_viewer_view_identity(result.identity)),
        file_ids: result
            .files
            .into_iter()
            .map(|file| file.as_str().to_owned())
            .collect(),
    }
}

pub fn decode_search_viewer_files_response(
    response: v1::SearchViewerFilesResponse,
) -> Result<ViewerFileSearchResult, ViewerCodecError> {
    Ok(ViewerFileSearchResult {
        identity: decode_viewer_view_identity(required(response.identity)?)?,
        files: response
            .file_ids
            .into_iter()
            .map(decode_viewer_diff_file_id)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

#[must_use]
pub fn encode_find_viewer_diff_request(request: FindViewerDiff) -> v1::FindViewerDiffRequest {
    v1::FindViewerDiffRequest {
        identity: Some(encode_viewer_view_identity(request.identity)),
        query: request.query,
        direction: encode_viewer_diff_search_direction(request.direction) as i32,
        anchor: request.anchor.as_ref().map(encode_viewer_diff_search_match),
        file_ids: request
            .files
            .into_iter()
            .map(|file| file.as_str().to_owned())
            .collect(),
    }
}

pub fn decode_find_viewer_diff_request(
    request: v1::FindViewerDiffRequest,
) -> Result<FindViewerDiff, ViewerCodecError> {
    Ok(FindViewerDiff {
        identity: decode_viewer_view_identity(required(request.identity)?)?,
        files: request
            .file_ids
            .into_iter()
            .map(decode_viewer_diff_file_id)
            .collect::<Result<Vec<_>, _>>()?,
        query: request.query,
        direction: decode_viewer_diff_search_direction(request.direction)?,
        anchor: request
            .anchor
            .map(decode_viewer_diff_search_match)
            .transpose()?,
    })
}

#[must_use]
pub fn encode_find_viewer_diff_response(
    result: &ViewerDiffSearchResult,
) -> v1::FindViewerDiffResponse {
    v1::FindViewerDiffResponse {
        identity: Some(encode_viewer_view_identity(result.identity)),
        total_matches: result.total_matches,
        active_match: result
            .active_match
            .as_ref()
            .map(encode_viewer_diff_search_match),
        wrapped: result.wrapped,
        matched_file_ids: result
            .matched_files
            .iter()
            .map(|file| file.as_str().to_owned())
            .collect(),
    }
}

pub fn decode_find_viewer_diff_response(
    response: v1::FindViewerDiffResponse,
) -> Result<ViewerDiffSearchResult, ViewerCodecError> {
    Ok(ViewerDiffSearchResult {
        identity: decode_viewer_view_identity(required(response.identity)?)?,
        total_matches: response.total_matches,
        active_match: response
            .active_match
            .map(decode_viewer_diff_search_match)
            .transpose()?,
        wrapped: response.wrapped,
        matched_files: response
            .matched_file_ids
            .into_iter()
            .map(decode_viewer_diff_file_id)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

const fn encode_viewer_diff_search_direction(
    direction: ViewerDiffSearchDirection,
) -> v1::ViewerDiffSearchDirection {
    match direction {
        ViewerDiffSearchDirection::Forward => v1::ViewerDiffSearchDirection::Forward,
        ViewerDiffSearchDirection::Backward => v1::ViewerDiffSearchDirection::Backward,
    }
}

fn decode_viewer_diff_search_direction(
    direction: i32,
) -> Result<ViewerDiffSearchDirection, ViewerCodecError> {
    match v1::ViewerDiffSearchDirection::try_from(direction) {
        Ok(v1::ViewerDiffSearchDirection::Forward) => Ok(ViewerDiffSearchDirection::Forward),
        Ok(v1::ViewerDiffSearchDirection::Backward) => Ok(ViewerDiffSearchDirection::Backward),
        Ok(v1::ViewerDiffSearchDirection::Unspecified) | Err(_) => {
            Err(ViewerCodecError::InvalidMessage)
        }
    }
}

fn encode_viewer_diff_search_match(found: &ViewerDiffSearchMatch) -> v1::ViewerDiffSearchMatch {
    v1::ViewerDiffSearchMatch {
        file_id: found.file.as_str().to_owned(),
        row_index: found.row_index,
    }
}

fn decode_viewer_diff_search_match(
    found: v1::ViewerDiffSearchMatch,
) -> Result<ViewerDiffSearchMatch, ViewerCodecError> {
    Ok(ViewerDiffSearchMatch {
        file: decode_viewer_diff_file_id(found.file_id)?,
        row_index: found.row_index,
    })
}

pub fn decode_watch_viewer_response(
    response: v1::WatchViewerResponse,
) -> Result<ViewerStateChanged, ViewerCodecError> {
    Ok(ViewerStateChanged {
        version: ViewerVersion::new(response.version),
        project_status: response
            .project_status
            .map(projects::decode_status_update)
            .transpose()?,
        live_check: response
            .live_check
            .map(|check| {
                Ok(crate::viewer::ViewerLiveCheck {
                    tab_id: ViewerTabId::try_new(check.tab_id)
                        .map_err(|_| ViewerCodecError::InvalidMessage)?,
                    result: match check.failure {
                        None => Ok(()),
                        Some(failure) => {
                            Err(decode_failure(failure).ok_or(ViewerCodecError::InvalidMessage)?)
                        }
                    },
                    elapsed_ms: check.elapsed_ms,
                })
            })
            .transpose()?,
    })
}

fn decode_viewer_shell(shell: v1::ViewerShell) -> Result<ViewerShell, ViewerCodecError> {
    let preferences = required(shell.preferences)?;
    Ok(ViewerShell {
        version: ViewerVersion::new(shell.version),
        focus_request_version: shell.focus_request_version.map(ViewerVersion::new),
        tabs: shell
            .tabs
            .into_iter()
            .map(decode_viewer_tab)
            .collect::<Result<Vec<_>, _>>()?,
        active: decode_viewer_active_state(required(shell.active)?)?,
        preferences: ViewerPreferences {
            copy_with_line_context: required(preferences.copy_with_line_context)?,
            diff_files_sort: decode_diff_files_sort(preferences.diff_files_sort)?,
            language: decode_viewer_language(preferences.language)?,
            date_format: decode_viewer_date_format(preferences.date_format)?,
            accessibility: decode_viewer_accessibility(required(preferences.accessibility)?)?,
            sidebars: decode_sidebar_visibility(required(preferences.sidebars)?),
            theme: decode_viewer_theme(preferences.theme)?,
            render_options: decode_viewer_render_options(required(preferences.render_options)?)?,
            keybindings: decode_viewer_keybindings(&required(preferences.keybindings)?)?,
        },
        feedback: shell.feedback.map(decode_viewer_feedback).transpose()?,
    })
}

fn encode_viewer_keybindings(keybindings: ViewerKeybindings) -> v1::ViewerKeybindings {
    v1::ViewerKeybindings {
        push_diff: Some(keybindings[ViewerKeybindingAction::PushDiff].to_string()),
        toggle_files_sidebar: keybindings[ViewerKeybindingAction::ToggleFilesSidebar].to_string(),
        toggle_commits_sidebar: keybindings[ViewerKeybindingAction::ToggleCommitsSidebar]
            .to_string(),
        platform: match keybindings.platform() {
            ViewerKeybindingPlatform::Linux => v1::ViewerKeybindingPlatform::Linux,
            ViewerKeybindingPlatform::Windows => v1::ViewerKeybindingPlatform::Windows,
            ViewerKeybindingPlatform::MacOs => v1::ViewerKeybindingPlatform::MacOs,
            ViewerKeybindingPlatform::Other => v1::ViewerKeybindingPlatform::Other,
        } as i32,
        search_files: keybindings[ViewerKeybindingAction::SearchFiles].to_string(),
        search_text_in_all_files: keybindings[ViewerKeybindingAction::SearchTextInAllFiles]
            .to_string(),
    }
}

fn decode_viewer_keybindings(
    keybindings: &v1::ViewerKeybindings,
) -> Result<ViewerKeybindings, ViewerCodecError> {
    let platform = match v1::ViewerKeybindingPlatform::try_from(keybindings.platform) {
        Ok(v1::ViewerKeybindingPlatform::Linux) => ViewerKeybindingPlatform::Linux,
        Ok(v1::ViewerKeybindingPlatform::Windows) => ViewerKeybindingPlatform::Windows,
        Ok(v1::ViewerKeybindingPlatform::MacOs) => ViewerKeybindingPlatform::MacOs,
        Ok(v1::ViewerKeybindingPlatform::Other) => ViewerKeybindingPlatform::Other,
        Ok(v1::ViewerKeybindingPlatform::Unspecified) | Err(_) => {
            return Err(ViewerCodecError::InvalidMessage);
        }
    };
    let search_files = keybindings
        .search_files
        .parse::<ViewerKeybinding>()
        .map_err(|_| ViewerCodecError::InvalidMessage)?;
    let search_text_in_all_files = keybindings
        .search_text_in_all_files
        .parse::<ViewerKeybinding>()
        .map_err(|_| ViewerCodecError::InvalidMessage)?;
    let toggle_files_sidebar = keybindings
        .toggle_files_sidebar
        .parse::<ViewerKeybinding>()
        .map_err(|_| ViewerCodecError::InvalidMessage)?;
    let toggle_commits_sidebar = keybindings
        .toggle_commits_sidebar
        .parse::<ViewerKeybinding>()
        .map_err(|_| ViewerCodecError::InvalidMessage)?;
    let push_diff = keybindings
        .push_diff
        .as_deref()
        .map(str::parse::<ViewerKeybinding>)
        .transpose()
        .map_err(|_| ViewerCodecError::InvalidMessage)?
        .unwrap_or(ViewerKeybindings::for_platform(platform)[ViewerKeybindingAction::PushDiff]);
    ViewerKeybindings::try_from_fn(platform, |action| match action {
        ViewerKeybindingAction::ToggleFilesSidebar => toggle_files_sidebar,
        ViewerKeybindingAction::ToggleCommitsSidebar => toggle_commits_sidebar,
        ViewerKeybindingAction::PushDiff => push_diff,
        ViewerKeybindingAction::SearchFiles => search_files,
        ViewerKeybindingAction::SearchTextInAllFiles => search_text_in_all_files,
    })
    .map_err(|_| ViewerCodecError::InvalidMessage)
}

fn decode_viewer_tab(tab: v1::ViewerTab) -> Result<ViewerTab, ViewerCodecError> {
    Ok(ViewerTab {
        details: tab.details.map(decode_viewer_tab_details).transpose()?,
        pinned: tab.pinned,
        custom_name: tab.custom_name,
        id: ViewerTabId::try_new(tab.id).map_err(|_| ViewerCodecError::InvalidMessage)?,
        label: recipe_label::decode(required(tab.recipe_label)?)?,
        live: tab.live,
        state: match v1::ViewerTabState::try_from(tab.state) {
            Ok(v1::ViewerTabState::Pending) => ViewerTabState::Pending,
            Ok(v1::ViewerTabState::Ready) => ViewerTabState::Ready,
            Ok(v1::ViewerTabState::Broken) => ViewerTabState::Broken,
            Ok(v1::ViewerTabState::Error) => ViewerTabState::Error,
            Ok(v1::ViewerTabState::Unspecified) | Err(_) => {
                return Err(ViewerCodecError::InvalidMessage);
            }
        },
    })
}

fn decode_viewer_tab_details(
    details: v1::ViewerTabDetails,
) -> Result<crate::viewer::ViewerTabDetails, ViewerCodecError> {
    Ok(crate::viewer::ViewerTabDetails {
        repository: gtl_models::paths::RepositoryRoot::try_new(details.repository.into())
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        comparison: recipe_label::decode(required(details.comparison)?)?,
        range: details
            .range
            .map(|range| {
                Ok::<_, ViewerCodecError>(gtl_models::diffs::PinnedRange {
                    base: CommitId::try_from(range.base)
                        .map_err(|_| ViewerCodecError::InvalidMessage)?,
                    head: CommitId::try_from(range.head)
                        .map_err(|_| ViewerCodecError::InvalidMessage)?,
                })
            })
            .transpose()?,
    })
}

fn decode_viewer_active_state(
    active: v1::ViewerActiveState,
) -> Result<ViewerActiveState, ViewerCodecError> {
    use v1::viewer_active_state::State;

    match required(active.state)? {
        State::Empty(_) => Ok(ViewerActiveState::Empty),
        State::Pending(state) => Ok(ViewerActiveState::Pending {
            tab_id: ViewerTabId::try_new(state.tab_id)
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
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
) -> Result<ViewerActiveState, ViewerCodecError> {
    let tab_id =
        ViewerTabId::try_new(failure.tab_id).map_err(|_| ViewerCodecError::InvalidMessage)?;
    let failure =
        decode_failure(required(failure.failure)?).ok_or(ViewerCodecError::InvalidMessage)?;
    Ok(if broken {
        ViewerActiveState::Broken { tab_id, failure }
    } else {
        ViewerActiveState::Error { tab_id, failure }
    })
}

fn decode_viewer_active_view(
    view: v1::ViewerActiveView,
) -> Result<ViewerActiveView, ViewerCodecError> {
    Ok(ViewerActiveView {
        modified_files: view.modified_files,
        identity: decode_viewer_view_identity(required(view.identity)?)?,
        row_source: match v1::ViewerRowSourceState::try_from(view.row_source)
            .map_err(|_| ViewerCodecError::InvalidMessage)?
        {
            v1::ViewerRowSourceState::Ready => crate::viewer::ViewerRowSourceState::Ready,
            v1::ViewerRowSourceState::Pending => crate::viewer::ViewerRowSourceState::Pending,
            v1::ViewerRowSourceState::Failed => crate::viewer::ViewerRowSourceState::Failed,
            v1::ViewerRowSourceState::Unspecified => return Err(ViewerCodecError::InvalidMessage),
        },
        content_id: crate::viewer::ViewerRowContentId::from_digest(
            required(view.content_id)?
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
        ),
        title: decode_diff_view_title(required(view.view_title)?)?,
        repository_name: ProjectName::try_new(view.repository_name)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        branch: GitHead::try_from(view.branch).map_err(|_| ViewerCodecError::InvalidMessage)?,
        upstream: GitRevision::try_new(view.upstream)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        command: decode_viewer_command_line(required(view.command)?),
        files: view
            .files
            .into_iter()
            .map(decode_viewer_file_summary)
            .collect::<Result<Vec<_>, _>>()?,
        commit_count: usize::try_from(view.commit_count)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        commits: Vec::new(),
        commit_selection: decode_viewer_commit_selection(required(view.commit_selection)?)?,
        footer: ViewerFooter {
            command: required(view.footer)?.command,
        },
        extension_filter: view
            .extension_filter
            .map(decode_viewer_applied_extension_filter)
            .transpose()?,
        changes_since: view
            .changes_since
            .map(|timestamp| {
                MachineTimestamp::try_from(timestamp).map_err(|_| ViewerCodecError::InvalidMessage)
            })
            .transpose()?,
    })
}

fn decode_viewer_file_summary(
    file: v1::ViewerFileSummary,
) -> Result<ViewerFileSummary, ViewerCodecError> {
    Ok(ViewerFileSummary {
        source_id: file
            .source_id
            .map(|value| {
                value
                    .try_into()
                    .map(crate::viewer::ViewerRowContentId::from_digest)
                    .map_err(|_| ViewerCodecError::InvalidMessage)
            })
            .transpose()?,
        id: file
            .id
            .try_into()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        path: RepositoryRelativePath::try_new(PathBuf::from(file.path))
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        absolute_path: AbsoluteFilePath::try_new(PathBuf::from(file.absolute_path))
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        anchor_id: file.anchor_id,
        added: DiffLineCount::new(u64::from(file.added)),
        removed: DiffLineCount::new(u64::from(file.removed)),
        status: match v1::ViewerFileStatus::try_from(file.status) {
            Ok(v1::ViewerFileStatus::Added) => ViewerFileStatus::Added,
            Ok(v1::ViewerFileStatus::Deleted) => ViewerFileStatus::Deleted,
            Ok(v1::ViewerFileStatus::Renamed) => ViewerFileStatus::Renamed,
            Ok(v1::ViewerFileStatus::Modified) => ViewerFileStatus::Modified,
            Ok(v1::ViewerFileStatus::Unspecified) | Err(_) => {
                return Err(ViewerCodecError::InvalidMessage);
            }
        },
        can_open_in_editor: file.can_open_in_editor,
        initially_expanded: file.initially_expanded,
        row_count: usize::try_from(file.row_count)
            .map_err(|_| ViewerCodecError::Unrepresentable)?,
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
) -> Result<ViewerCommitSelection, ViewerCodecError> {
    let commit_id = || {
        selection
            .commit_id
            .clone()
            .ok_or(ViewerCodecError::InvalidMessage)?
            .try_into()
            .map_err(|_| ViewerCodecError::InvalidMessage)
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
            failure: selection
                .failure
                .and_then(decode_failure)
                .ok_or(ViewerCodecError::InvalidMessage)?,
        }),
        Ok(v1::ViewerCommitSelectionState::Unspecified) | Err(_) => {
            Err(ViewerCodecError::InvalidMessage)
        }
    }
}

fn decode_viewer_applied_extension_filter(
    applied: v1::ViewerAppliedExtensionFilter,
) -> Result<ViewerAppliedExtensionFilter, ViewerCodecError> {
    Ok(ViewerAppliedExtensionFilter {
        filter: file_filters::decode_extension_filter(required(applied.filter)?)?,
        hidden_paths: applied
            .hidden_paths
            .into_iter()
            .map(|path| {
                RepositoryRelativePath::try_new(PathBuf::from(path))
                    .map_err(|_| ViewerCodecError::InvalidMessage)
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn decode_viewer_feedback(
    feedback: v1::ViewerFeedback,
) -> Result<ViewerFeedback, ViewerCodecError> {
    match v1::ViewerFeedbackKind::try_from(feedback.kind) {
        Ok(v1::ViewerFeedbackKind::SnapshotRecipesSkipped)
            if !feedback.recipe_labels.is_empty() =>
        {
            Ok(ViewerFeedback::SnapshotRecipesSkipped {
                labels: feedback
                    .recipe_labels
                    .into_iter()
                    .map(recipe_label::decode)
                    .collect::<Result<_, _>>()?,
            })
        }
        Ok(
            v1::ViewerFeedbackKind::SnapshotRecipesSkipped | v1::ViewerFeedbackKind::Unspecified,
        )
        | Err(_) => Err(ViewerCodecError::InvalidMessage),
    }
}

pub fn decode_viewer_view_identity(
    identity: v1::ViewerViewIdentity,
) -> Result<ViewerViewIdentity, ViewerCodecError> {
    Ok(ViewerViewIdentity {
        tab_id: ViewerTabId::try_new(identity.tab_id)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        range_generation: ViewerRangeGeneration::new(identity.range_generation),
        selection_generation: ViewerSelectionGeneration::new(identity.selection_generation),
        render_options: decode_viewer_render_options(required(identity.render_options)?)?,
    })
}

#[must_use]
pub fn encode_viewer_view_identity(identity: ViewerViewIdentity) -> v1::ViewerViewIdentity {
    v1::ViewerViewIdentity {
        tab_id: identity.tab_id.into(),
        range_generation: identity.range_generation.value(),
        selection_generation: identity.selection_generation.value(),
        render_options: Some(encode_viewer_render_options(identity.render_options)),
    }
}

pub fn decode_viewer_render_options(
    options: v1::ViewerRenderOptions,
) -> Result<ViewerRenderOptions, ViewerCodecError> {
    Ok(ViewerRenderOptions {
        wrap_lines: options.wrap_lines,
        layout: decode_viewer_diff_layout(options.layout)?,
        density: decode_viewer_diff_density(options.density)?,
    })
}

pub fn decode_viewer_diff_layout(layout: i32) -> Result<ViewerDiffLayout, ViewerCodecError> {
    match v1::ViewerDiffLayout::try_from(layout) {
        Ok(v1::ViewerDiffLayout::Unified) => Ok(ViewerDiffLayout::Unified),
        Ok(v1::ViewerDiffLayout::Split) => Ok(ViewerDiffLayout::Split),
        Ok(v1::ViewerDiffLayout::Unspecified) | Err(_) => Err(ViewerCodecError::InvalidMessage),
    }
}

pub fn decode_viewer_diff_density(density: i32) -> Result<ViewerDiffDensity, ViewerCodecError> {
    match v1::ViewerDiffDensity::try_from(density) {
        Ok(v1::ViewerDiffDensity::Compact) => Ok(ViewerDiffDensity::Compact),
        Ok(v1::ViewerDiffDensity::Full) => Ok(ViewerDiffDensity::Full),
        Ok(v1::ViewerDiffDensity::Unspecified) | Err(_) => Err(ViewerCodecError::InvalidMessage),
    }
}

#[must_use]
pub fn encode_viewer_render_options(options: ViewerRenderOptions) -> v1::ViewerRenderOptions {
    v1::ViewerRenderOptions {
        wrap_lines: options.wrap_lines,
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

pub fn decode_viewer_theme(theme: i32) -> Result<ViewerTheme, ViewerCodecError> {
    match v1::ViewerTheme::try_from(theme) {
        Ok(v1::ViewerTheme::Dark) => Ok(ViewerTheme::Dark),
        Ok(v1::ViewerTheme::Glacier) => Ok(ViewerTheme::Glacier),
        Ok(v1::ViewerTheme::Mirage) => Ok(ViewerTheme::Mirage),
        Ok(v1::ViewerTheme::Graphite) => Ok(ViewerTheme::Graphite),
        Ok(v1::ViewerTheme::Carbon) => Ok(ViewerTheme::Carbon),
        Ok(v1::ViewerTheme::Unspecified) | Err(_) => Err(ViewerCodecError::InvalidMessage),
    }
}

#[must_use]
pub const fn encode_viewer_theme(theme: ViewerTheme) -> v1::ViewerTheme {
    match theme {
        ViewerTheme::Dark => v1::ViewerTheme::Dark,
        ViewerTheme::Mirage => v1::ViewerTheme::Mirage,
        ViewerTheme::Glacier => v1::ViewerTheme::Glacier,
        ViewerTheme::Graphite => v1::ViewerTheme::Graphite,
        ViewerTheme::Carbon => v1::ViewerTheme::Carbon,
    }
}

fn decode_viewer_commit_summary(
    commit: v1::ViewerCommitSummary,
) -> Result<ViewerCommitSummary, ViewerCodecError> {
    Ok(ViewerCommitSummary {
        id: CommitId::try_from(commit.id).map_err(|_| ViewerCodecError::InvalidMessage)?,
        subject: commit.subject,
        body: commit.body,
        committed_at: MachineTimestamp::try_from(commit.committed_at.as_str())
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        is_merge: commit.is_merge,
    })
}

fn encode_viewer_history_position(
    render_id: RenderHistoryId,
    page: HistoryPageNumber,
) -> Result<v1::ViewerHistoryPosition, ViewerCodecError> {
    Ok(v1::ViewerHistoryPosition {
        render_id: encode_render_history_id(render_id)?,
        page: page.into(),
    })
}

fn decode_viewer_history_entry(
    entry: v1::ViewerHistoryEntry,
) -> Result<ViewerHistoryEntry, ViewerCodecError> {
    let raw_id = i64::try_from(entry.id).map_err(|_| ViewerCodecError::InvalidMessage)?;
    Ok(ViewerHistoryEntry {
        id: RenderHistoryId::try_new(raw_id).map_err(|_| ViewerCodecError::InvalidMessage)?,
        label: recipe_label::decode(required(entry.recipe_label)?)?,
        repository_name: ProjectName::try_new(entry.repository_name)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        kind: match v1::ViewerRecipeKind::try_from(entry.kind) {
            Ok(v1::ViewerRecipeKind::Diff) => ViewerRecipeKind::Diff,
            Ok(v1::ViewerRecipeKind::MergeDiff) => ViewerRecipeKind::MergeDiff,
            Ok(v1::ViewerRecipeKind::Unspecified) | Err(_) => {
                return Err(ViewerCodecError::InvalidMessage);
            }
        },
        range_label: entry.range_label,
        rendered_at: MachineTimestamp::try_from(entry.rendered_at.as_str())
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
    })
}

fn encode_render_history_id(render_id: RenderHistoryId) -> Result<u64, ViewerCodecError> {
    u64::try_from(i64::from(render_id)).map_err(|_| ViewerCodecError::Unrepresentable)
}

fn decode_render_history_id(render_id: u64) -> Result<RenderHistoryId, ViewerCodecError> {
    let render_id = i64::try_from(render_id).map_err(|_| ViewerCodecError::InvalidMessage)?;
    RenderHistoryId::try_new(render_id).map_err(|_| ViewerCodecError::InvalidMessage)
}

fn decode_viewer_row_event(
    event: v1::stream_viewer_rows_response::Event,
) -> Result<ViewerRowEvent, ViewerCodecError> {
    use v1::stream_viewer_rows_response::Event;

    match event {
        Event::FileStarted(event) => Ok(ViewerRowEvent::FileStarted {
            file: decode_viewer_diff_file_id(event.file_id)?,
            row_count: event.row_count,
            start_row: event.start_row,
        }),
        Event::UnifiedRows(event) => Ok(ViewerRowEvent::UnifiedRows {
            file: decode_viewer_diff_file_id(event.file_id)?,
            start_row: event.start_row,
            rows: event
                .rows
                .into_iter()
                .map(decode_viewer_unified_row)
                .collect::<Result<Vec<_>, _>>()?,
        }),
        Event::SplitRows(event) => Ok(ViewerRowEvent::SplitRows {
            file: decode_viewer_diff_file_id(event.file_id)?,
            start_row: event.start_row,
            rows: event
                .rows
                .into_iter()
                .map(decode_viewer_split_row)
                .collect::<Result<Vec<_>, _>>()?,
        }),
        Event::FileFinished(event) => Ok(ViewerRowEvent::FileFinished {
            file: decode_viewer_diff_file_id(event.file_id)?,
            line_number_digits: event.line_number_digits,
            end_row: event.end_row,
        }),
        Event::FileFailed(event) => Ok(ViewerRowEvent::FileFailed {
            file: decode_viewer_diff_file_id(event.file_id)?,
            failure: event
                .failure
                .and_then(decode_failure)
                .ok_or(ViewerCodecError::InvalidMessage)?,
            retryable: event.retryable,
        }),
    }
}

fn decode_viewer_diff_file_id(
    file_id: String,
) -> Result<crate::viewer::ViewerDiffFileId, ViewerCodecError> {
    file_id
        .try_into()
        .map_err(|_| ViewerCodecError::InvalidMessage)
}

/// Encodes one unified diff row for a streamed viewer response.
pub fn encode_viewer_unified_row(
    row: ViewerUnifiedRow,
) -> Result<v1::ViewerUnifiedRow, ViewerCodecError> {
    let row = match row {
        ViewerUnifiedRow::Meta(text) => v1::viewer_unified_row::Row::Meta(text),
        ViewerUnifiedRow::Hunk(text) => v1::viewer_unified_row::Row::Hunk(text),
        ViewerUnifiedRow::Context(row) => {
            v1::viewer_unified_row::Row::Context(encode_viewer_unified_source_row(row)?)
        }
        ViewerUnifiedRow::Added(row) => {
            v1::viewer_unified_row::Row::Added(encode_viewer_unified_source_row(row)?)
        }
        ViewerUnifiedRow::Removed(row) => {
            v1::viewer_unified_row::Row::Removed(encode_viewer_unified_source_row(row)?)
        }
    };
    Ok(v1::ViewerUnifiedRow { row: Some(row) })
}

fn encode_viewer_unified_source_row(
    row: ViewerUnifiedSourceRow,
) -> Result<v1::ViewerUnifiedSourceRow, ViewerCodecError> {
    Ok(v1::ViewerUnifiedSourceRow {
        old_line_number: row.old_line_number,
        new_line_number: row.new_line_number,
        code: Some(encode_viewer_code_line(row.code)?),
    })
}

/// Encodes one split diff row for a streamed viewer response.
pub fn encode_viewer_split_row(
    row: ViewerSplitRow,
) -> Result<v1::ViewerSplitRow, ViewerCodecError> {
    let row = match row {
        ViewerSplitRow::Meta(text) => v1::viewer_split_row::Row::Meta(text),
        ViewerSplitRow::Hunk(text) => v1::viewer_split_row::Row::Hunk(text),
        ViewerSplitRow::Context {
            old_line_number,
            new_line_number,
            code,
        } => v1::viewer_split_row::Row::Context(v1::ViewerSplitContextRow {
            old_line_number,
            new_line_number,
            code: Some(encode_viewer_code_line(code)?),
        }),
        ViewerSplitRow::Pair { old, new } => {
            v1::viewer_split_row::Row::Pair(v1::ViewerSplitPairRow {
                old: old.map(encode_viewer_split_cell).transpose()?,
                new: new.map(encode_viewer_split_cell).transpose()?,
            })
        }
    };
    Ok(v1::ViewerSplitRow { row: Some(row) })
}

fn encode_viewer_split_cell(
    cell: ViewerSplitCell,
) -> Result<v1::ViewerSplitCell, ViewerCodecError> {
    Ok(v1::ViewerSplitCell {
        line_number: cell.line_number,
        code: Some(encode_viewer_code_line(cell.code)?),
    })
}

fn encode_viewer_code_line(line: ViewerCodeLine) -> Result<v1::ViewerCodeLine, ViewerCodecError> {
    let mut next_byte = 0_usize;
    let spans = line
        .spans
        .into_iter()
        .map(|span| {
            if span.byte_start != next_byte
                || span.byte_end < span.byte_start
                || line.text.get(span.byte_start..span.byte_end).is_none()
            {
                return Err(ViewerCodecError::Unrepresentable);
            }
            let projected = v1::ViewerCodeSpan {
                byte_start: u32::try_from(span.byte_start)
                    .map_err(|_| ViewerCodecError::Unrepresentable)?,
                byte_end: u32::try_from(span.byte_end)
                    .map_err(|_| ViewerCodecError::Unrepresentable)?,
                syntax_class: span
                    .syntax_class
                    .map_or(v1::ViewerSyntaxClass::Unspecified as i32, |class| {
                        encode_viewer_syntax_class(class) as i32
                    }),
                changed: span.changed,
            };
            next_byte = span.byte_end;
            Ok(projected)
        })
        .collect::<Result<Vec<_>, ViewerCodecError>>()?;
    if next_byte != line.text.len() {
        return Err(ViewerCodecError::Unrepresentable);
    }
    Ok(v1::ViewerCodeLine {
        text: line.text,
        spans,
        omitted_character_count: line
            .omitted_character_count
            .map(u32::try_from)
            .transpose()
            .map_err(|_| ViewerCodecError::Unrepresentable)?,
    })
}

const fn encode_viewer_syntax_class(class: ViewerSyntaxClass) -> v1::ViewerSyntaxClass {
    match class {
        ViewerSyntaxClass::Keyword => v1::ViewerSyntaxClass::Keyword,
        ViewerSyntaxClass::String => v1::ViewerSyntaxClass::String,
        ViewerSyntaxClass::Comment => v1::ViewerSyntaxClass::Comment,
        ViewerSyntaxClass::Type => v1::ViewerSyntaxClass::Type,
        ViewerSyntaxClass::Function => v1::ViewerSyntaxClass::Function,
        ViewerSyntaxClass::Number => v1::ViewerSyntaxClass::Number,
        ViewerSyntaxClass::Constant => v1::ViewerSyntaxClass::Constant,
        ViewerSyntaxClass::Operator => v1::ViewerSyntaxClass::Operator,
        ViewerSyntaxClass::Tag => v1::ViewerSyntaxClass::Tag,
        ViewerSyntaxClass::Variable => v1::ViewerSyntaxClass::Variable,
    }
}

fn decode_viewer_unified_row(
    row: v1::ViewerUnifiedRow,
) -> Result<ViewerUnifiedRow, ViewerCodecError> {
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
) -> Result<ViewerUnifiedSourceRow, ViewerCodecError> {
    Ok(ViewerUnifiedSourceRow {
        old_line_number: row.old_line_number,
        new_line_number: row.new_line_number,
        code: decode_viewer_code_line(required(row.code)?)?,
    })
}

fn decode_viewer_split_row(row: v1::ViewerSplitRow) -> Result<ViewerSplitRow, ViewerCodecError> {
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
) -> Result<ViewerSplitCell, ViewerCodecError> {
    Ok(ViewerSplitCell {
        line_number: cell.line_number,
        code: decode_viewer_code_line(required(cell.code)?)?,
    })
}

fn decode_viewer_code_line(line: v1::ViewerCodeLine) -> Result<ViewerCodeLine, ViewerCodecError> {
    let mut next_byte = 0_usize;
    let mut spans = Vec::with_capacity(line.spans.len().max(1));
    for span in line.spans {
        let byte_start =
            usize::try_from(span.byte_start).map_err(|_| ViewerCodecError::InvalidMessage)?;
        let byte_end =
            usize::try_from(span.byte_end).map_err(|_| ViewerCodecError::InvalidMessage)?;
        if byte_start != next_byte || byte_end < byte_start {
            return Err(ViewerCodecError::InvalidMessage);
        }
        line.text
            .get(byte_start..byte_end)
            .ok_or(ViewerCodecError::InvalidMessage)?;
        spans.push(ViewerCodeSpan {
            byte_start,
            byte_end,
            syntax_class: decode_viewer_syntax_class(span.syntax_class)?,
            changed: span.changed,
        });
        next_byte = byte_end;
    }
    if next_byte != line.text.len() {
        if !spans.is_empty() {
            return Err(ViewerCodecError::InvalidMessage);
        }
        spans.push(ViewerCodeSpan {
            byte_start: 0,
            byte_end: line.text.len(),
            syntax_class: None,
            changed: false,
        });
    }
    Ok(ViewerCodeLine {
        text: line.text,
        spans,
        omitted_character_count: line
            .omitted_character_count
            .map(usize::try_from)
            .transpose()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
    })
}

fn decode_viewer_syntax_class(
    syntax_class: i32,
) -> Result<Option<ViewerSyntaxClass>, ViewerCodecError> {
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
        Err(_) => Err(ViewerCodecError::InvalidMessage),
    }
}

fn required<T>(value: Option<T>) -> Result<T, ViewerCodecError> {
    value.ok_or(ViewerCodecError::InvalidMessage)
}

#[must_use]
pub fn decode_get_settings_recovery_response(
    response: v1::GetSettingsRecoveryResponse,
) -> crate::viewer::ViewerSettingsRecovery {
    crate::viewer::ViewerSettingsRecovery {
        configuration_path: response.configuration_path,
        diagnostic: response.diagnostic,
        revision: response.revision,
    }
}

#[must_use]
pub fn encode_reset_settings_request(
    request: crate::viewer::ResetSettings,
) -> v1::ResetSettingsRequest {
    v1::ResetSettingsRequest {
        revision: request.revision,
    }
}

#[must_use]
pub fn decode_reset_settings_response(
    response: v1::ResetSettingsResponse,
) -> crate::viewer::ResetSettingsOk {
    crate::viewer::ResetSettingsOk {
        backup_path: response.backup_path,
    }
}

fn encode_sidebar_visibility(
    sidebars: gtl_models::viewer::ViewerSidebarVisibility,
) -> v1::ViewerSidebarVisibility {
    v1::ViewerSidebarVisibility {
        files: sidebars.files,
        commits: sidebars.commits,
    }
}

fn decode_sidebar_visibility(
    sidebars: v1::ViewerSidebarVisibility,
) -> gtl_models::viewer::ViewerSidebarVisibility {
    gtl_models::viewer::ViewerSidebarVisibility {
        files: sidebars.files,
        commits: sidebars.commits,
    }
}

fn decode_bool_field_update(
    update: Option<v1::BoolFieldUpdate>,
) -> Result<FieldUpdate<bool>, ViewerCodecError> {
    Ok(match update {
        None => FieldUpdate::Unchanged,
        Some(update) => match required(update.operation)? {
            v1::bool_field_update::Operation::Update(value) => FieldUpdate::Update(value),
            v1::bool_field_update::Operation::Clear(_) => FieldUpdate::Clear,
        },
    })
}

#[must_use]
pub fn encode_set_viewer_modified_files_request(
    request: crate::viewer::SetViewerModifiedFiles,
) -> v1::SetViewerModifiedFilesRequest {
    v1::SetViewerModifiedFilesRequest {
        tab_id: request.tab_id.into(),
        visible: request.visible,
    }
}

#[must_use]
pub fn encode_set_viewer_tab_pinned_request(
    request: crate::viewer::SetViewerTabPinned,
) -> v1::SetViewerTabPinnedRequest {
    v1::SetViewerTabPinnedRequest {
        tab_id: u64::from(request.tab_id),
        pinned: request.pinned,
    }
}
#[must_use]
pub fn encode_set_viewer_tab_live_request(
    request: crate::viewer::SetViewerTabLive,
) -> v1::SetViewerTabLiveRequest {
    v1::SetViewerTabLiveRequest {
        tab_id: u64::from(request.tab_id),
        live: request.live,
    }
}

#[must_use]
pub fn encode_set_viewer_changes_since_request(
    request: crate::viewer::SetViewerChangesSince,
) -> v1::SetViewerChangesSinceRequest {
    v1::SetViewerChangesSinceRequest {
        tab_id: u64::from(request.tab_id),
        changes_since: request.changes_since.map(|timestamp| timestamp.to_string()),
    }
}

pub fn decode_set_viewer_changes_since_request(
    request: v1::SetViewerChangesSinceRequest,
) -> Result<crate::viewer::SetViewerChangesSince, ViewerCodecError> {
    Ok(crate::viewer::SetViewerChangesSince {
        tab_id: ViewerTabId::try_new(request.tab_id)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        changes_since: request
            .changes_since
            .map(|timestamp| {
                MachineTimestamp::try_from(timestamp).map_err(|_| ViewerCodecError::InvalidMessage)
            })
            .transpose()
            .map_err(field("changes_since"))?,
    })
}

pub fn decode_set_viewer_changes_since_response(
    response: v1::SetViewerChangesSinceResponse,
) -> Result<ViewerShell, ViewerCodecError> {
    decode_viewer_shell(required(response.shell)?)
}

pub fn decode_set_viewer_tab_live_response(
    response: v1::SetViewerTabLiveResponse,
) -> Result<ViewerShell, ViewerCodecError> {
    decode_viewer_shell(required(response.shell)?)
}

#[must_use]
pub fn encode_close_other_viewer_tabs_request(
    request: crate::viewer::ViewerTabRequest,
) -> v1::CloseOtherViewerTabsRequest {
    v1::CloseOtherViewerTabsRequest {
        tab_id: u64::from(request.tab_id),
    }
}

fn encode_projects_page_size_update(
    update: &FieldUpdate<gtl_models::settings::ProjectsPageSize>,
) -> Option<v1::ProjectsPageSizeFieldUpdate> {
    let operation = match update {
        FieldUpdate::Unchanged => return None,
        FieldUpdate::Clear => {
            v1::projects_page_size_field_update::Operation::Clear(v1::ClearSetting {})
        }
        FieldUpdate::Update(value) => {
            v1::projects_page_size_field_update::Operation::Update(value.into_inner())
        }
    };
    Some(v1::ProjectsPageSizeFieldUpdate {
        operation: Some(operation),
    })
}

pub mod file_filters;

fn decode_projects_sort(
    value: i32,
) -> Result<gtl_models::settings::ProjectsSort, ViewerCodecError> {
    match v1::ProjectsSort::try_from(value) {
        Ok(v1::ProjectsSort::Changes) => Ok(gtl_models::settings::ProjectsSort::Changes),
        Ok(v1::ProjectsSort::Name) => Ok(gtl_models::settings::ProjectsSort::Name),
        Ok(v1::ProjectsSort::Branch) => Ok(gtl_models::settings::ProjectsSort::Branch),
        Ok(v1::ProjectsSort::ChangesAscending) => {
            Ok(gtl_models::settings::ProjectsSort::ChangesAscending)
        }
        Ok(v1::ProjectsSort::NameDescending) => {
            Ok(gtl_models::settings::ProjectsSort::NameDescending)
        }
        Ok(v1::ProjectsSort::BranchDescending) => {
            Ok(gtl_models::settings::ProjectsSort::BranchDescending)
        }
        _ => Err(ViewerCodecError::InvalidMessage),
    }
}

const fn encode_projects_sort(value: gtl_models::settings::ProjectsSort) -> v1::ProjectsSort {
    match value {
        gtl_models::settings::ProjectsSort::Changes => v1::ProjectsSort::Changes,
        gtl_models::settings::ProjectsSort::Name => v1::ProjectsSort::Name,
        gtl_models::settings::ProjectsSort::Branch => v1::ProjectsSort::Branch,
        gtl_models::settings::ProjectsSort::ChangesAscending => v1::ProjectsSort::ChangesAscending,
        gtl_models::settings::ProjectsSort::NameDescending => v1::ProjectsSort::NameDescending,
        gtl_models::settings::ProjectsSort::BranchDescending => v1::ProjectsSort::BranchDescending,
    }
}

fn encode_projects_sort_update(
    update: &FieldUpdate<gtl_models::settings::ProjectsSort>,
) -> Option<v1::ProjectsSortFieldUpdate> {
    let operation = match update {
        FieldUpdate::Unchanged => return None,
        FieldUpdate::Clear => v1::projects_sort_field_update::Operation::Clear(v1::ClearSetting {}),
        FieldUpdate::Update(value) => {
            v1::projects_sort_field_update::Operation::Update(encode_projects_sort(*value) as i32)
        }
    };
    Some(v1::ProjectsSortFieldUpdate {
        operation: Some(operation),
    })
}

fn decode_diff_files_sort(
    value: i32,
) -> Result<gtl_models::settings::DiffFilesSort, ViewerCodecError> {
    match v1::DiffFilesSort::try_from(value) {
        Ok(v1::DiffFilesSort::Path) => Ok(gtl_models::settings::DiffFilesSort::Path),
        Ok(v1::DiffFilesSort::Changes) => Ok(gtl_models::settings::DiffFilesSort::Changes),
        Ok(v1::DiffFilesSort::Unspecified) | Err(_) => Err(ViewerCodecError::InvalidMessage),
    }
}

const fn encode_diff_files_sort(value: gtl_models::settings::DiffFilesSort) -> v1::DiffFilesSort {
    match value {
        gtl_models::settings::DiffFilesSort::Path => v1::DiffFilesSort::Path,
        gtl_models::settings::DiffFilesSort::Changes => v1::DiffFilesSort::Changes,
    }
}

fn encode_diff_files_sort_update(
    update: &FieldUpdate<gtl_models::settings::DiffFilesSort>,
) -> Option<v1::DiffFilesSortFieldUpdate> {
    let operation = match update {
        FieldUpdate::Unchanged => return None,
        FieldUpdate::Clear => {
            v1::diff_files_sort_field_update::Operation::Clear(v1::ClearSetting {})
        }
        FieldUpdate::Update(value) => v1::diff_files_sort_field_update::Operation::Update(
            encode_diff_files_sort(*value) as i32,
        ),
    };
    Some(v1::DiffFilesSortFieldUpdate {
        operation: Some(operation),
    })
}

#[must_use]
pub fn encode_rename_viewer_snapshot_request(
    request: crate::viewer::RenameViewerSnapshot,
) -> v1::RenameViewerSnapshotRequest {
    v1::RenameViewerSnapshotRequest {
        tab_id: u64::from(request.tab_id),
        name: request.name,
    }
}

fn decode_project_names(
    names: Vec<String>,
) -> Result<Vec<gtl_models::paths::ProjectName>, ViewerCodecError> {
    if names.len() > 1000 {
        return Err(ViewerCodecError::InvalidMessage);
    }
    names
        .into_iter()
        .map(|name| {
            gtl_models::paths::ProjectName::try_new(name)
                .map_err(|_| ViewerCodecError::InvalidMessage)
        })
        .collect()
}

fn encode_project_names_update(
    value: FieldUpdate<Vec<gtl_models::paths::ProjectName>>,
) -> Option<v1::ViewerProjectNamesFieldUpdate> {
    use v1::viewer_project_names_field_update::Operation;
    let operation = match value {
        FieldUpdate::Update(names) => Operation::Update(v1::ViewerProjectNames {
            values: names.into_iter().map(|name| name.to_string()).collect(),
        }),
        FieldUpdate::Clear => Operation::Clear(v1::ClearSetting {}),
        FieldUpdate::Unchanged => return None,
    };
    Some(v1::ViewerProjectNamesFieldUpdate {
        operation: Some(operation),
    })
}

fn decode_project_names_update(
    value: Option<v1::ViewerProjectNamesFieldUpdate>,
) -> Result<FieldUpdate<Vec<gtl_models::paths::ProjectName>>, ViewerCodecError> {
    use v1::viewer_project_names_field_update::Operation;
    Ok(match value {
        None => FieldUpdate::Unchanged,
        Some(value) => match required(value.operation)? {
            Operation::Update(names) => FieldUpdate::Update(decode_project_names(names.values)?),
            Operation::Clear(_) => FieldUpdate::Clear,
        },
    })
}

#[must_use]
pub fn encode_search_viewer_commits_request(
    request: crate::viewer::commit_search::SearchViewerCommits,
) -> v1::SearchViewerCommitsRequest {
    use v1::search_viewer_commits_request::Scope;

    use crate::viewer::commit_search::ViewerCommitSearchScope;
    v1::SearchViewerCommitsRequest {
        scope: Some(match request.scope {
            ViewerCommitSearchScope::Snapshot(identity) => {
                Scope::Snapshot(encode_viewer_view_identity(identity))
            }
            ViewerCommitSearchScope::ActiveBranch(path) => {
                Scope::ActiveBranchPath(path.to_string())
            }
            ViewerCommitSearchScope::ActiveBranchSnapshot(identity) => {
                Scope::ActiveBranchSnapshot(encode_viewer_view_identity(identity))
            }
        }),
        query: request.query,
        time_from: request.time_range.from().map(ToString::to_string),
        time_until: request.time_range.until().map(ToString::to_string),
    }
}

pub fn decode_search_viewer_commits_request(
    request: v1::SearchViewerCommitsRequest,
) -> Result<crate::viewer::commit_search::SearchViewerCommits, ViewerCodecError> {
    use v1::search_viewer_commits_request::Scope;

    use crate::viewer::commit_search::{SearchViewerCommits, ViewerCommitSearchScope};
    if request.query.len() > crate::viewer::VIEWER_SEARCH_QUERY_MAX_BYTES {
        return Err(ViewerCodecError::InvalidField { field: "query" });
    }
    let scope = match required(request.scope)? {
        Scope::Snapshot(identity) => {
            ViewerCommitSearchScope::Snapshot(decode_viewer_view_identity(identity)?)
        }
        Scope::ActiveBranchSnapshot(identity) => {
            ViewerCommitSearchScope::ActiveBranchSnapshot(decode_viewer_view_identity(identity)?)
        }
        Scope::ActiveBranchPath(path) => ViewerCommitSearchScope::ActiveBranch(
            gtl_models::paths::RepositoryRoot::try_new(path.into())
                .map_err(|_| ViewerCodecError::InvalidField { field: "path" })?,
        ),
    };
    Ok(SearchViewerCommits {
        scope,
        query: request.query,
        time_range: CommitTimeRange::new(
            request
                .time_from
                .map(MachineTimestamp::try_from)
                .transpose()
                .map_err(|_| ViewerCodecError::InvalidField { field: "time_from" })?,
            request
                .time_until
                .map(MachineTimestamp::try_from)
                .transpose()
                .map_err(|_| ViewerCodecError::InvalidField {
                    field: "time_until",
                })?,
        )
        .map_err(|_| ViewerCodecError::InvalidField {
            field: "time_range",
        })?,
    })
}

#[must_use]
pub fn encode_search_viewer_commits_response(
    result: crate::viewer::commit_search::ViewerCommitSearchResult,
) -> v1::SearchViewerCommitsResponse {
    v1::SearchViewerCommitsResponse {
        commits: result
            .commits
            .into_iter()
            .map(|commit| v1::ViewerCommitSummary {
                id: commit.id.to_string(),
                subject: commit.subject,
                body: commit.body,
                committed_at: commit.committed_at.to_string(),
                is_merge: commit.is_merge,
                body_omitted: true,
            })
            .collect(),
        total_matches: result.total_matches,
        branch: result.branch.map(|branch| branch.to_string()),
    }
}

pub fn decode_search_viewer_commits_response(
    result: v1::SearchViewerCommitsResponse,
) -> Result<crate::viewer::commit_search::ViewerCommitSearchResult, ViewerCodecError> {
    Ok(crate::viewer::commit_search::ViewerCommitSearchResult {
        commits: result
            .commits
            .into_iter()
            .map(decode_viewer_commit_summary)
            .collect::<Result<_, _>>()?,
        total_matches: result.total_matches,
        branch: result
            .branch
            .map(|branch| {
                branch
                    .try_into()
                    .map_err(|_| ViewerCodecError::InvalidMessage)
            })
            .transpose()?,
    })
}

#[must_use]
pub fn encode_open_viewer_commit_request(
    request: crate::viewer::commit_search::OpenViewerCommit,
) -> v1::OpenViewerCommitRequest {
    use v1::open_viewer_commit_request::Scope;

    use crate::viewer::commit_search::ViewerCommitSearchScope;
    v1::OpenViewerCommitRequest {
        scope: Some(match request.scope {
            ViewerCommitSearchScope::Snapshot(identity) => {
                Scope::Snapshot(encode_viewer_view_identity(identity))
            }
            ViewerCommitSearchScope::ActiveBranch(path) => {
                Scope::ActiveBranchPath(path.to_string())
            }
            ViewerCommitSearchScope::ActiveBranchSnapshot(identity) => {
                Scope::ActiveBranchSnapshot(encode_viewer_view_identity(identity))
            }
        }),
        id: request.id.to_string(),
    }
}

pub fn decode_open_viewer_commit_request(
    request: v1::OpenViewerCommitRequest,
) -> Result<crate::viewer::commit_search::OpenViewerCommit, ViewerCodecError> {
    use v1::open_viewer_commit_request::Scope;

    use crate::viewer::commit_search::ViewerCommitSearchScope;
    let scope = match required(request.scope)? {
        Scope::Snapshot(identity) => {
            ViewerCommitSearchScope::Snapshot(decode_viewer_view_identity(identity)?)
        }
        Scope::ActiveBranchSnapshot(identity) => {
            ViewerCommitSearchScope::ActiveBranchSnapshot(decode_viewer_view_identity(identity)?)
        }
        Scope::ActiveBranchPath(path) => ViewerCommitSearchScope::ActiveBranch(
            gtl_models::paths::RepositoryRoot::try_new(path.into())
                .map_err(|_| ViewerCodecError::InvalidField { field: "path" })?,
        ),
    };
    Ok(crate::viewer::commit_search::OpenViewerCommit {
        scope,
        id: request
            .id
            .try_into()
            .map_err(|_| ViewerCodecError::InvalidField { field: "id" })?,
    })
}

pub fn decode_open_viewer_commit_response(
    response: v1::OpenViewerCommitResponse,
) -> Result<crate::viewer::projects::OpenViewerProjectOk, ViewerCodecError> {
    Ok(crate::viewer::projects::OpenViewerProjectOk {
        tab_id: ViewerTabId::try_from(response.tab_id)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
    })
}

#[cfg(test)]
mod commit_search_tests {
    use super::*;
    use crate::viewer::commit_search::{
        OpenViewerCommit, SearchViewerCommits, ViewerCommitSearchScope,
    };

    #[test]
    fn scoped_commit_requests_round_trip_and_reject_missing_or_oversized_input() {
        let identity = ViewerViewIdentity {
            tab_id: ViewerTabId::try_from(1_u64).unwrap(),
            range_generation: ViewerRangeGeneration::new(1),
            selection_generation: ViewerSelectionGeneration::default(),
            render_options: crate::viewer::ViewerRenderOptions {
                wrap_lines: false,
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        };
        for scope in [
            ViewerCommitSearchScope::ActiveBranch(
                gtl_models::paths::RepositoryRoot::try_new(
                    "//fixture.invalid/repositories/repo".into(),
                )
                .unwrap(),
            ),
            ViewerCommitSearchScope::Snapshot(identity),
            ViewerCommitSearchScope::ActiveBranchSnapshot(identity),
        ] {
            let request = SearchViewerCommits {
                scope: scope.clone(),
                query: "ação".into(),
                time_range: CommitTimeRange::new(
                    Some("2026-09-29T09:00:00-03:00".try_into().unwrap()),
                    Some("2026-09-30T12:00:00Z".try_into().unwrap()),
                )
                .unwrap(),
            };
            assert_eq!(
                decode_search_viewer_commits_request(encode_search_viewer_commits_request(
                    request.clone()
                ))
                .unwrap(),
                request
            );
            let request = OpenViewerCommit {
                scope,
                id: "a".repeat(40).try_into().unwrap(),
            };
            assert_eq!(
                decode_open_viewer_commit_request(encode_open_viewer_commit_request(
                    request.clone()
                ))
                .unwrap(),
                request
            );
        }
        assert!(
            decode_search_viewer_commits_request(v1::SearchViewerCommitsRequest::default())
                .is_err()
        );
        assert!(decode_open_viewer_commit_request(v1::OpenViewerCommitRequest::default()).is_err());
        assert_eq!(
            decode_search_viewer_commits_request(v1::SearchViewerCommitsRequest {
                query: "x".repeat(257),
                ..Default::default()
            })
            .unwrap_err(),
            ViewerCodecError::InvalidField { field: "query" }
        );
    }

    #[test]
    fn time_bounds_allow_missing_values_and_reject_invalid_or_reversed_instants() {
        let request = || v1::SearchViewerCommitsRequest {
            scope: Some(v1::search_viewer_commits_request::Scope::ActiveBranchPath(
                "//fixture.invalid/repositories/repo".into(),
            )),
            ..Default::default()
        };
        for (from, until) in [
            (None, None),
            (Some("2026-09-29T12:00:00Z"), None),
            (None, Some("2026-09-30T12:00:00Z")),
        ] {
            let raw = v1::SearchViewerCommitsRequest {
                time_from: from.map(str::to_owned),
                time_until: until.map(str::to_owned),
                ..request()
            };
            assert_eq!(
                encode_search_viewer_commits_request(
                    decode_search_viewer_commits_request(raw.clone()).unwrap()
                ),
                raw
            );
        }
        for (from, until, field) in [
            ("2026-09-29T12:00:00", "2026-09-30T12:00:00Z", "time_from"),
            ("2026-09-29T12:00:00Z", "tomorrow", "time_until"),
            ("2026-09-30T12:00:00Z", "2026-09-29T12:00:00Z", "time_range"),
        ] {
            assert_eq!(
                decode_search_viewer_commits_request(v1::SearchViewerCommitsRequest {
                    time_from: Some(from.into()),
                    time_until: Some(until.into()),
                    ..request()
                })
                .unwrap_err(),
                ViewerCodecError::InvalidField { field }
            );
        }
    }
}
