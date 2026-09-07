//! Projects the server-owned viewer session into its client-facing shell.

use std::sync::Arc;

use gtl_models::{
    diffs::Commit,
    paths::RepositoryRelativePath,
    viewer::{self, RenderOptions, Theme, ViewerKeybindings},
};
use gtl_wire::viewer::{
    ViewerActiveState, ViewerActiveView, ViewerCommitSelection, ViewerDiffFileId,
    ViewerFailureCode, ViewerFeedback, ViewerPreferences, ViewerShell, ViewerTab, ViewerTabKind,
    ViewerTabState, ViewerViewIdentity,
};

use super::{
    ViewerState, ViewerStateError, project_diff_view, project_render_options, project_theme,
};
use crate::viewer::session::{
    ActiveContentIdentity, ActiveContentSnapshot, CommitSelectionSnapshot, RENDER_PENDING_REASON,
    ViewerSession,
};

const SOURCE_DIRECTORY_NOT_FOUND_MESSAGE: &str =
    "The configured Git repository directory was not found. Restore it and refresh.";
const SOURCE_NOT_GIT_REPOSITORY_MESSAGE: &str =
    "The configured directory is not a Git repository. Restore the repository and refresh.";
const SOURCE_UNAVAILABLE_MESSAGE: &str =
    "The live view source is unavailable. Restore it and refresh.";
const RENDER_FAILED_MESSAGE: &str = "The diff could not be rendered. Please retry.";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProjectViewerShellError {
    #[error("the active viewer tab is missing")]
    ActiveTabMissing,
    #[error("the active viewer view is unavailable")]
    ActiveViewUnavailable,
}

/// Projects one short-lived snapshot of the viewer shell.
pub fn project(
    session: &mut ViewerSession,
    options: RenderOptions,
    theme: Theme,
    keybindings: ViewerKeybindings,
    feedback: Option<ViewerFeedback>,
) -> Result<ViewerShell, ProjectViewerShellError> {
    let tabs = session
        .tabs()
        .map(|entry| ViewerTab {
            id: entry.tab.id(),
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
                .ok_or(ProjectViewerShellError::ActiveTabMissing)?;
            match non_ready_active_state(tab_id, &state) {
                Some(active) => active,
                None => ViewerActiveState::Ready {
                    view: Box::new(ready_active_view(session, tab_id, options)?),
                },
            }
        }
    };
    Ok(ViewerShell {
        version: session.version(),
        focus_request_version: session.focus_request_version(),
        tabs,
        active,
        preferences: ViewerPreferences {
            theme: project_theme(theme),
            render_options: project_render_options(options),
            keybindings,
        },
        feedback,
    })
}

fn ready_active_view(
    session: &mut ViewerSession,
    tab_id: gtl_models::viewer::ViewerTabId,
    options: RenderOptions,
) -> Result<ViewerActiveView, ProjectViewerShellError> {
    let identity = session
        .active_displayed_content_identity()
        .ok_or(ProjectViewerShellError::ActiveViewUnavailable)?;
    let range = session
        .cached_view_snapshot(tab_id)
        .ok_or(ProjectViewerShellError::ActiveViewUnavailable)?
        .view;
    let selection = session.commit_selection_snapshot(tab_id);
    let displayed = match &selection {
        CommitSelectionSnapshot::Ready { view, .. } => Arc::clone(view),
        CommitSelectionSnapshot::None
        | CommitSelectionSnapshot::Pending { .. }
        | CommitSelectionSnapshot::Error { .. } => Arc::clone(&range),
    };
    let commit_selection = match &selection {
        CommitSelectionSnapshot::None => ViewerCommitSelection::None,
        CommitSelectionSnapshot::Pending { id } => {
            ViewerCommitSelection::Pending { id: id.clone() }
        }
        CommitSelectionSnapshot::Ready { id, .. } => {
            ViewerCommitSelection::Ready { id: id.clone() }
        }
        CommitSelectionSnapshot::Error { id, reason } => ViewerCommitSelection::Error {
            id: id.clone(),
            message: reason.clone(),
        },
    };
    Ok(project_diff_view(
        &displayed,
        &range,
        identity_for(identity, options),
        commit_selection,
    ))
}

#[must_use]
pub fn identity_for(identity: ActiveContentIdentity, options: RenderOptions) -> ViewerViewIdentity {
    ViewerViewIdentity {
        tab_id: identity.tab_id(),
        range_generation: identity.range_generation(),
        selection_generation: identity.selection_generation(),
        render_options: project_render_options(options),
    }
}

#[must_use]
pub fn identity_matches(
    expected: ViewerViewIdentity,
    actual: ActiveContentIdentity,
    options: RenderOptions,
) -> bool {
    expected == identity_for(actual, options)
}

pub fn content_snapshot_for_identity(
    state: &ViewerState,
    identity: ViewerViewIdentity,
    options: RenderOptions,
) -> Result<Option<ActiveContentSnapshot>, ViewerStateError> {
    state.inspect(|session| {
        let current = session.active_content_identity()?;
        identity_matches(identity, current, options)
            .then(|| session.active_content_snapshot())
            .flatten()
    })
}

pub fn commits_for_identity(
    state: &ViewerState,
    identity: ViewerViewIdentity,
    options: RenderOptions,
) -> Result<Option<Vec<Commit>>, ViewerStateError> {
    state.inspect(|session| {
        let current = session.active_displayed_content_identity()?;
        if !identity_matches(identity, current, options) {
            return None;
        }
        session
            .cached_view_snapshot(identity.tab_id)
            .map(|cached| cached.view.commits.clone())
    })
}

pub fn diff_file_for_identity(
    state: &ViewerState,
    identity: ViewerViewIdentity,
    options: RenderOptions,
    file_id: &ViewerDiffFileId,
) -> Result<Option<(Arc<crate::diffs::View>, RepositoryRelativePath)>, ViewerStateError> {
    Ok(
        content_snapshot_for_identity(state, identity, options)?.and_then(|snapshot| {
            snapshot
                .view()
                .files
                .iter()
                .enumerate()
                .find(|(index, _)| ViewerDiffFileId::for_index(*index) == *file_id)
                .map(|(_, file)| (snapshot.shared_view(), file.path.clone()))
        }),
    )
}

pub fn identity_is_current(
    state: &ViewerState,
    identity: ViewerViewIdentity,
    options: RenderOptions,
) -> Result<bool, ViewerStateError> {
    state.inspect(|session| {
        session
            .active_content_identity()
            .is_some_and(|current| identity_matches(identity, current, options))
    })
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

fn broken_failure(code: &str) -> (ViewerFailureCode, &'static str) {
    match code {
        "DirNotFound" => (
            ViewerFailureCode::RepositoryDirectoryNotFound,
            SOURCE_DIRECTORY_NOT_FOUND_MESSAGE,
        ),
        "DirNotGitRepo" => (
            ViewerFailureCode::RepositoryDirectoryNotGitRepository,
            SOURCE_NOT_GIT_REPOSITORY_MESSAGE,
        ),
        _ => (
            ViewerFailureCode::SourceUnavailable,
            SOURCE_UNAVAILABLE_MESSAGE,
        ),
    }
}

fn non_ready_active_state(
    tab_id: gtl_models::viewer::ViewerTabId,
    state: &viewer::ViewerTabState,
) -> Option<ViewerActiveState> {
    match state {
        viewer::ViewerTabState::Ready => None,
        viewer::ViewerTabState::Error { reason } if reason == RENDER_PENDING_REASON => {
            Some(ViewerActiveState::Pending { tab_id })
        }
        viewer::ViewerTabState::Broken { code, .. } => {
            let (code, message) = broken_failure(code);
            Some(ViewerActiveState::Broken {
                tab_id,
                code,
                message: message.into(),
            })
        }
        viewer::ViewerTabState::Error { .. } => Some(ViewerActiveState::Error {
            tab_id,
            code: ViewerFailureCode::RenderFailed,
            message: RENDER_FAILED_MESSAGE.into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_projection_does_not_expose_backend_details() {
        let state = viewer::ViewerTabState::Error {
            reason: "git failed in /private/repository".into(),
        };
        let tab_id = gtl_models::viewer::ViewerTabId::try_new(7).unwrap();

        let projected = non_ready_active_state(tab_id, &state).unwrap();
        let json = serde_json::to_string(&projected).unwrap();

        assert!(!json.contains("/private/repository"));
        assert!(json.contains(RENDER_FAILED_MESSAGE));
    }
}
