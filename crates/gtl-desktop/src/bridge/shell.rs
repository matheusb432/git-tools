use std::sync::Arc;

use gtl_application::diffs::{FileStatus, View};
use gtl_contracts::viewer::{
    ViewerActiveState, ViewerActiveView, ViewerAppliedExclusions, ViewerCommandLine,
    ViewerCommitSelection, ViewerCommitSummary, ViewerFailureCode, ViewerFeedback,
    ViewerFileStatus, ViewerFileSummary, ViewerFooter, ViewerPreferences, ViewerRenderOptions,
    ViewerShell, ViewerTab, ViewerTabKind, ViewerTabState, ViewerTheme, ViewerViewIdentity,
};
use gtl_models::viewer::{self, DiffDensity, DiffLayout, RenderOptions, Theme};

use super::{internal, settings};
use crate::{
    live_view_restoration,
    presentation::ViewerApp,
    session::{
        ActiveContentIdentity, CommitSelectionSnapshot, RENDER_PENDING_REASON, ViewerSession,
    },
};

pub(super) fn load(
    app: &ViewerApp,
    feedback: Option<ViewerFeedback>,
) -> Result<ViewerShell, gtl_contracts::viewer::ViewerApiError> {
    live_view_restoration::restore(app)
        .map_err(|error| internal("failed to restore saved live views", error))?;
    ensure_active_cache(app)?;
    let user_settings = settings::load(app)?;
    let options = user_settings.viewer_render_options();
    let theme = user_settings.theme().unwrap_or(Theme::Dark);
    let mut session = app
        .session
        .lock()
        .map_err(|error| internal("failed to lock viewer session", error))?;
    project(&mut session, options, theme, feedback)
}

fn ensure_active_cache(app: &ViewerApp) -> Result<(), gtl_contracts::viewer::ViewerApiError> {
    let refresh = {
        let mut session = app
            .session
            .lock()
            .map_err(|error| internal("failed to lock viewer session", error))?;
        let active = session.active();
        active.filter(|id| {
            matches!(
                session.tab(*id).map(|tab| tab.tab.state()),
                Some(viewer::ViewerTabState::Ready)
            ) && session.cached_view_snapshot(*id).is_none()
        })
    };
    if let Some(tab_id) = refresh {
        app.refresh_recipe(tab_id)
            .map_err(|error| internal("failed to restore an evicted active view", error))?;
    }
    Ok(())
}

fn project(
    session: &mut ViewerSession,
    options: RenderOptions,
    theme: Theme,
    feedback: Option<ViewerFeedback>,
) -> Result<ViewerShell, gtl_contracts::viewer::ViewerApiError> {
    let revision = session.revision();
    let tabs = session
        .tabs()
        .map(|entry| ViewerTab {
            id: entry.tab.id().into(),
            label: entry.tab.label().to_owned(),
            kind: to_tab_kind(entry.tab.kind()),
            state: to_tab_state(entry.tab.state()),
        })
        .collect();
    let active = match session.active() {
        None => ViewerActiveState::Empty,
        Some(tab_id) => {
            let state = session
                .tab(tab_id)
                .map(|entry| entry.tab.state().clone())
                .ok_or_else(|| internal("active viewer tab is missing", tab_id))?;
            match to_non_ready_active_state(tab_id, &state) {
                Some(active) => active,
                None => ready_active_view(session, tab_id, options).map(|view| {
                    ViewerActiveState::Ready {
                        view: Box::new(view),
                    }
                })?,
            }
        }
    };
    Ok(ViewerShell {
        revision,
        tabs,
        active,
        preferences: ViewerPreferences {
            theme: to_theme(theme),
            render_options: to_render_options(options),
        },
        feedback,
    })
}

fn ready_active_view(
    session: &mut ViewerSession,
    tab_id: gtl_models::viewer::ViewerTabId,
    options: RenderOptions,
) -> Result<ViewerActiveView, gtl_contracts::viewer::ViewerApiError> {
    let identity = session
        .active_displayed_content_identity()
        .ok_or(gtl_contracts::viewer::ViewerApiError::Conflict)?;
    let range = session
        .cached_view_snapshot(tab_id)
        .ok_or(gtl_contracts::viewer::ViewerApiError::Conflict)?
        .view;
    let selection = session.commit_selection_snapshot(tab_id);
    let displayed = match &selection {
        CommitSelectionSnapshot::Ready { view, .. } => Arc::clone(view),
        CommitSelectionSnapshot::None
        | CommitSelectionSnapshot::Pending { .. }
        | CommitSelectionSnapshot::Error { .. } => Arc::clone(&range),
    };
    Ok(to_active_view(
        &displayed,
        &range,
        to_identity(identity, options),
        &selection,
    ))
}

fn to_active_view(
    view: &View,
    range_view: &View,
    identity: ViewerViewIdentity,
    selection: &CommitSelectionSnapshot,
) -> ViewerActiveView {
    ViewerActiveView {
        identity,
        title: view.title.clone(),
        repository_name: view.repo_name.clone(),
        branch: view.branch.clone(),
        upstream: view.upstream.clone(),
        command: ViewerCommandLine {
            lead: view.cmd.lead.clone(),
            range: view.cmd.range.clone(),
            trail: view.cmd.trail.clone(),
        },
        files: view
            .files
            .iter()
            .map(|file| {
                let status = file.status();
                ViewerFileSummary {
                    path: file.path.clone(),
                    anchor_id: gtl_preview::diff_file_anchor_id(&file.path),
                    added: file.added,
                    removed: file.removed,
                    status: to_file_status(status),
                    can_open_in_editor: status != FileStatus::Deleted,
                }
            })
            .collect(),
        commits_label: range_view.commits_label.clone(),
        commits: range_view
            .commits
            .iter()
            .map(|commit| ViewerCommitSummary {
                sha: commit.sha.clone(),
                abbreviated_sha: commit.sha.chars().take(10).collect(),
                subject: commit.subject.clone(),
                body: commit.body.clone(),
                date: commit.date.clone(),
                iso: commit.iso.clone(),
                is_merge: commit.is_merge(),
            })
            .collect(),
        commit_selection: match selection {
            CommitSelectionSnapshot::None => ViewerCommitSelection::None,
            CommitSelectionSnapshot::Pending { sha } => {
                ViewerCommitSelection::Pending { sha: sha.clone() }
            }
            CommitSelectionSnapshot::Ready { sha, .. } => {
                ViewerCommitSelection::Ready { sha: sha.clone() }
            }
            CommitSelectionSnapshot::Error { sha, reason } => ViewerCommitSelection::Error {
                sha: sha.clone(),
                message: reason.clone(),
            },
        },
        footer: ViewerFooter {
            command: view.foot.cmd.clone(),
            note: view.foot.note.clone(),
        },
        exclusions: view
            .exclusions
            .as_ref()
            .map(|exclusions| ViewerAppliedExclusions {
                extensions: exclusions.extensions.clone(),
                hidden_paths: exclusions.hidden_paths.clone(),
            }),
    }
}

pub(super) fn to_identity(
    identity: ActiveContentIdentity,
    options: RenderOptions,
) -> ViewerViewIdentity {
    ViewerViewIdentity {
        tab_id: identity.tab_id().into(),
        range_generation: identity.range_generation(),
        selection_generation: identity.selection_generation(),
        render_options: to_render_options(options),
    }
}

pub(super) fn identity_matches(
    expected: ViewerViewIdentity,
    actual: ActiveContentIdentity,
    options: RenderOptions,
) -> bool {
    expected == to_identity(actual, options)
}

pub(super) const fn to_render_options(options: RenderOptions) -> ViewerRenderOptions {
    ViewerRenderOptions {
        layout: match options.layout() {
            DiffLayout::Unified => gtl_contracts::viewer::ViewerDiffLayout::Unified,
            DiffLayout::Split => gtl_contracts::viewer::ViewerDiffLayout::Split,
        },
        density: match options.density() {
            DiffDensity::Compact => gtl_contracts::viewer::ViewerDiffDensity::Compact,
            DiffDensity::Full => gtl_contracts::viewer::ViewerDiffDensity::Full,
        },
    }
}

#[cfg(test)]
const fn from_render_options(options: ViewerRenderOptions) -> RenderOptions {
    RenderOptions::new(
        match options.layout {
            gtl_contracts::viewer::ViewerDiffLayout::Unified => DiffLayout::Unified,
            gtl_contracts::viewer::ViewerDiffLayout::Split => DiffLayout::Split,
        },
        match options.density {
            gtl_contracts::viewer::ViewerDiffDensity::Compact => DiffDensity::Compact,
            gtl_contracts::viewer::ViewerDiffDensity::Full => DiffDensity::Full,
        },
    )
}

pub(super) const fn to_theme(theme: Theme) -> ViewerTheme {
    match theme {
        Theme::Dark => ViewerTheme::Dark,
        Theme::Light => ViewerTheme::Light,
        Theme::Hearth => ViewerTheme::Hearth,
        Theme::Mirage => ViewerTheme::Mirage,
        Theme::Glacier => ViewerTheme::Glacier,
        Theme::Noir => ViewerTheme::Noir,
        Theme::Graphite => ViewerTheme::Graphite,
    }
}

pub(super) const fn from_theme(theme: ViewerTheme) -> Theme {
    match theme {
        ViewerTheme::Dark => Theme::Dark,
        ViewerTheme::Light => Theme::Light,
        ViewerTheme::Hearth => Theme::Hearth,
        ViewerTheme::Mirage => Theme::Mirage,
        ViewerTheme::Glacier => Theme::Glacier,
        ViewerTheme::Noir => Theme::Noir,
        ViewerTheme::Graphite => Theme::Graphite,
    }
}

const fn to_tab_kind(kind: viewer::ViewerTabKind) -> ViewerTabKind {
    match kind {
        viewer::ViewerTabKind::Snapshot => ViewerTabKind::Snapshot,
        viewer::ViewerTabKind::Live => ViewerTabKind::Live,
    }
}

fn to_tab_state(state: &viewer::ViewerTabState) -> ViewerTabState {
    match state {
        viewer::ViewerTabState::Ready => ViewerTabState::Ready,
        viewer::ViewerTabState::Broken { .. } => ViewerTabState::Broken,
        viewer::ViewerTabState::Error { reason } if reason == RENDER_PENDING_REASON => {
            ViewerTabState::Pending
        }
        viewer::ViewerTabState::Error { .. } => ViewerTabState::Error,
    }
}

const SOURCE_DIRECTORY_NOT_FOUND_MESSAGE: &str =
    "The configured Git repository directory was not found. Restore it and refresh.";
const SOURCE_NOT_GIT_REPOSITORY_MESSAGE: &str =
    "The configured directory is not a Git repository. Restore the repository and refresh.";
const SOURCE_UNAVAILABLE_MESSAGE: &str =
    "The live view source is unavailable. Restore it and refresh.";
const RENDER_FAILED_MESSAGE: &str = "The diff could not be rendered. Please retry.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PublicFailure {
    code: ViewerFailureCode,
    message: &'static str,
}

fn broken_failure(code: &str) -> PublicFailure {
    match code {
        "DirNotFound" => PublicFailure {
            code: ViewerFailureCode::RepositoryDirectoryNotFound,
            message: SOURCE_DIRECTORY_NOT_FOUND_MESSAGE,
        },
        "DirNotGitRepo" => PublicFailure {
            code: ViewerFailureCode::RepositoryDirectoryNotGitRepository,
            message: SOURCE_NOT_GIT_REPOSITORY_MESSAGE,
        },
        _ => PublicFailure {
            code: ViewerFailureCode::SourceUnavailable,
            message: SOURCE_UNAVAILABLE_MESSAGE,
        },
    }
}

fn to_non_ready_active_state(
    tab_id: gtl_models::viewer::ViewerTabId,
    state: &viewer::ViewerTabState,
) -> Option<ViewerActiveState> {
    let tab_id = tab_id.into();
    match state {
        viewer::ViewerTabState::Ready => None,
        viewer::ViewerTabState::Error { reason } if reason == RENDER_PENDING_REASON => {
            Some(ViewerActiveState::Pending { tab_id })
        }
        viewer::ViewerTabState::Broken { code, .. } => {
            let failure = broken_failure(code);
            Some(ViewerActiveState::Broken {
                tab_id,
                code: failure.code,
                message: failure.message.into(),
            })
        }
        viewer::ViewerTabState::Error { .. } => Some(ViewerActiveState::Error {
            tab_id,
            code: ViewerFailureCode::RenderFailed,
            message: RENDER_FAILED_MESSAGE.into(),
        }),
    }
}

const fn to_file_status(status: FileStatus) -> ViewerFileStatus {
    match status {
        FileStatus::Added => ViewerFileStatus::Added,
        FileStatus::Deleted => ViewerFileStatus::Deleted,
        FileStatus::Renamed => ViewerFileStatus::Renamed,
        FileStatus::Modified => ViewerFileStatus::Modified,
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::diffs::{Cmd, FileDiff, Foot};
    use gtl_models::diffs::{AppliedExclusions, Commit};

    use super::*;

    fn view() -> View {
        View {
            repo_name: "git-tools".into(),
            repo_root: "/repo".into(),
            branch: "feature".into(),
            upstream: "origin/main".into(),
            commits: vec![Commit {
                sha: "0123456789abcdef0123456789abcdef01234567".into(),
                subject: "subject".into(),
                parents: vec!["one".into(), "two".into()],
                ..Commit::default()
            }],
            files: vec![
                FileDiff {
                    path: "src/a b.rs".into(),
                    added: 2,
                    removed: 1,
                    lines: vec!["new file mode 100644".into()],
                    full_lines: None,
                },
                FileDiff {
                    path: "removed.rs".into(),
                    added: 0,
                    removed: 1,
                    lines: vec!["deleted file mode 100644".into()],
                    full_lines: None,
                },
            ],
            title: "Feature diff".into(),
            cmd: Cmd {
                lead: "git diff ".into(),
                range: "main...feature".into(),
                trail: " --".into(),
            },
            commits_label: "2 commits".into(),
            foot: Foot {
                cmd: "git diff".into(),
                note: "generated".into(),
            },
            exclusions: Some(AppliedExclusions {
                extensions: vec!["lock".into()],
                hidden_paths: vec!["Cargo.lock".into()],
            }),
        }
    }

    #[test]
    fn active_view_mapping_uses_the_renderer_anchor_and_range_commit_shelf() {
        let identity = ViewerViewIdentity {
            tab_id: 7,
            range_generation: 8,
            selection_generation: 9,
            render_options: to_render_options(RenderOptions::DEFAULT),
        };
        let active = to_active_view(&view(), &view(), identity, &CommitSelectionSnapshot::None);

        assert_eq!(active.identity.tab_id, 7);
        assert_eq!(active.files[0].anchor_id, "f-src-a-b-rs");
        assert!(active.files[0].can_open_in_editor);
        assert!(!active.files[1].can_open_in_editor);
        assert_eq!(active.commits[0].abbreviated_sha, "0123456789");
        assert!(active.commits[0].is_merge);
        assert_eq!(
            active.exclusions.expect("applied exclusions").hidden_paths,
            ["Cargo.lock"]
        );
    }

    #[test]
    fn render_option_mapping_round_trips_every_variant_pair() {
        for options in [
            RenderOptions::new(DiffLayout::Unified, DiffDensity::Compact),
            RenderOptions::new(DiffLayout::Unified, DiffDensity::Full),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Compact),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
        ] {
            assert_eq!(from_render_options(to_render_options(options)), options);
        }
    }

    #[test]
    fn broken_projection_never_serializes_the_source_path_or_probe_reason() {
        let raw_reason =
            "The git repo's directory at `/home/alice/private/customer-repo` was not found.";
        let state = viewer::ViewerTabState::Broken {
            code: "DirNotFound".into(),
            reason: raw_reason.into(),
        };
        let tab = to_tab_state(&state);
        let tab_id = gtl_models::viewer::ViewerTabId::try_new(7);
        assert!(tab_id.is_ok());
        let Ok(tab_id) = tab_id else {
            return;
        };
        let active = to_non_ready_active_state(tab_id, &state);
        assert!(active.is_some());
        let Some(active) = active else {
            return;
        };
        let payload = serde_json::to_string(&(tab, active));
        assert!(payload.is_ok());
        let Ok(payload) = payload else {
            return;
        };

        assert!(!payload.contains("/home/alice"));
        assert!(!payload.contains(raw_reason));
        assert!(payload.contains("DirNotFound"));
        assert!(payload.contains("was not found"));
    }

    #[test]
    fn error_projection_never_serializes_raw_backend_diagnostics() {
        let raw_reason = "git exited 128 while reading /srv/secret/repo: permission denied";
        let state = viewer::ViewerTabState::Error {
            reason: raw_reason.into(),
        };
        let tab = to_tab_state(&state);
        let tab_id = gtl_models::viewer::ViewerTabId::try_new(7);
        assert!(tab_id.is_ok());
        let Ok(tab_id) = tab_id else {
            return;
        };
        let active = to_non_ready_active_state(tab_id, &state);
        assert!(active.is_some());
        let Some(active) = active else {
            return;
        };
        let payload = serde_json::to_string(&(tab, active));
        assert!(payload.is_ok());
        let Ok(payload) = payload else {
            return;
        };

        assert!(!payload.contains("/srv/secret"));
        assert!(!payload.contains(raw_reason));
        assert!(payload.contains("RenderFailed"));
        assert!(payload.contains("The diff could not be rendered. Please retry."));
    }

    #[test]
    fn public_failure_messages_are_fixed_and_bounded() {
        const MAX_MESSAGE_BYTES: usize = 96;
        let failures = [
            broken_failure("DirNotFound"),
            broken_failure("DirNotGitRepo"),
            broken_failure("unexpected-backend-code"),
            PublicFailure {
                code: ViewerFailureCode::RenderFailed,
                message: RENDER_FAILED_MESSAGE,
            },
        ];

        assert!(
            failures
                .iter()
                .all(|failure| failure.message.len() <= MAX_MESSAGE_BYTES)
        );
        assert_eq!(
            failures[2].code,
            ViewerFailureCode::SourceUnavailable,
            "unknown backend codes collapse to one public fallback"
        );
    }
}
