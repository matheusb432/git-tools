//! Projects the server-owned viewer session into its client-facing shell.

use std::sync::Arc;

use gtl_models::{
    failure::ErrorMeta,
    paths::RepositoryRelativePath,
    settings::UserSettings,
    viewer::{self, RenderOptions},
};
use gtl_wire::viewer::{
    ViewerActiveState, ViewerActiveView, ViewerCommitSelection, ViewerDiffFileId, ViewerFeedback,
    ViewerPreferences, ViewerShell, ViewerTab, ViewerTabState, ViewerViewIdentity,
};

use super::{
    ViewerState, ViewerStateError, diff_view::project_diff_view_with_content_id,
    project_render_options, project_theme,
};
use crate::viewer::session::{
    ActiveContentIdentity, ActiveContentSnapshot, CommitSelectionSnapshot, ViewerSession,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq, ErrorMeta)]
pub enum ProjectViewerShellError {
    #[error("the active viewer tab is missing")]
    #[meta(private(Internal))]
    ActiveTabMissing,
    #[error("the active viewer view is unavailable")]
    #[meta(private(Internal))]
    ActiveViewUnavailable,
}

/// Projects one short-lived snapshot of the viewer shell, reporting snapshots
/// skipped since the previous projection.
pub fn project(
    session: &mut ViewerSession,
    settings: &UserSettings,
) -> Result<ViewerShell, ProjectViewerShellError> {
    let options = settings.viewer_render_options();
    let tabs = session
        .tabs()
        .map(|entry| ViewerTab {
            details: Some(session.tab_details(entry)),
            pinned: entry.pinned,
            custom_name: entry.recipe.name.as_ref().map(ToString::to_string),
            id: entry.tab.id(),
            label: entry.tab.label().clone(),
            live: entry.tab.live(),
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
            let modified = session.modified_files_snapshot(tab_id).is_some();
            match if modified {
                None
            } else {
                non_ready_active_state(tab_id, &state)
            } {
                Some(active) => active,
                None => ViewerActiveState::Ready {
                    view: Box::new(ready_active_view(session, tab_id, options)?),
                },
            }
        }
    };
    let skipped = session.take_finished_skipped_snapshots();
    let feedback =
        (!skipped.is_empty()).then_some(ViewerFeedback::SnapshotRecipesSkipped { labels: skipped });
    Ok(ViewerShell {
        version: session.version(),
        focus_request_version: session.focus_request_version(),
        tabs,
        active,
        preferences: ViewerPreferences {
            accessibility: settings.accessibility(),
            language: settings.language(),
            date_format: settings.date_format(),
            sidebars: settings.sidebar_visibility(),
            theme: project_theme(settings.theme().unwrap_or_default()),
            render_options: project_render_options(options),
            copy_with_line_context: settings.copy_with_line_context(),
            diff_files_sort: settings.diff_files_sort(),
            keybindings: settings.viewer_keybindings(),
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
        .map(|cached| cached.view)
        .or_else(|| session.modified_files_snapshot(tab_id))
        .ok_or(ProjectViewerShellError::ActiveViewUnavailable)?;
    let selection = session.commit_selection_snapshot(tab_id);
    let displayed = match &selection {
        CommitSelectionSnapshot::Ready { view, .. } => view.clone(),
        CommitSelectionSnapshot::None
        | CommitSelectionSnapshot::Pending { .. }
        | CommitSelectionSnapshot::Error { .. } => range.clone(),
    };
    let modified = session.modified_files_snapshot(tab_id);
    let modified_files = modified.is_some();
    let displayed = modified.unwrap_or(displayed);
    let displayed = session.full_context_snapshot(identity).unwrap_or(displayed);
    let commit_selection = match &selection {
        CommitSelectionSnapshot::None => ViewerCommitSelection::None,
        CommitSelectionSnapshot::Pending { id } => {
            ViewerCommitSelection::Pending { id: id.clone() }
        }
        CommitSelectionSnapshot::Ready { id, .. } => {
            ViewerCommitSelection::Ready { id: id.clone() }
        }
        CommitSelectionSnapshot::Error { id, failure } => ViewerCommitSelection::Error {
            id: id.clone(),
            failure: failure.clone(),
        },
    };
    let commit_selection = if modified_files {
        ViewerCommitSelection::None
    } else {
        commit_selection
    };
    let mut view = project_diff_view_with_content_id(
        &displayed,
        &range,
        identity_for(identity, options),
        commit_selection,
        displayed.content_id(project_render_options(options)),
    );
    if let Some(name) = session.tab(tab_id).and_then(|tab| tab.recipe.name.as_ref()) {
        view.title = gtl_models::diffs::DiffViewTitle::Named { name: name.clone() };
    }
    view.modified_files = modified_files;
    view.changes_since = session.tab_changes_since(tab_id);
    if options.density() == super::DiffDensity::Full
        && matches!(
            displayed.full_context,
            crate::diffs::FullContextDiffState::Deferred(_)
        )
    {
        view.row_source = if session.full_context_failed(identity) {
            gtl_wire::viewer::ViewerRowSourceState::Failed
        } else {
            gtl_wire::viewer::ViewerRowSourceState::Pending
        };
    }
    Ok(view)
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

pub fn commit_source_for_identity(
    state: &ViewerState,
    identity: ViewerViewIdentity,
    options: RenderOptions,
) -> Result<Option<super::ViewerDiffSnapshot>, ViewerStateError> {
    state.inspect(|session| {
        let current = session.active_displayed_content_identity()?;
        if !identity_matches(identity, current, options) {
            return None;
        }
        session
            .cached_view_snapshot(identity.tab_id)
            .map(|cached| cached.view)
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

pub fn tab_identity_is_current(
    state: &ViewerState,
    identity: ViewerViewIdentity,
    options: RenderOptions,
) -> Result<bool, ViewerStateError> {
    state.inspect(|session| {
        session
            .content_identity(identity.tab_id)
            .is_some_and(|current| identity_matches(identity, current, options))
    })
}

fn to_tab_state(state: &viewer::ViewerTabState) -> ViewerTabState {
    match state {
        viewer::ViewerTabState::Pending => ViewerTabState::Pending,
        viewer::ViewerTabState::Ready => ViewerTabState::Ready,
        viewer::ViewerTabState::Broken { .. } => ViewerTabState::Broken,
        viewer::ViewerTabState::Error { .. } => ViewerTabState::Error,
    }
}

fn non_ready_active_state(
    tab_id: gtl_models::viewer::ViewerTabId,
    state: &viewer::ViewerTabState,
) -> Option<ViewerActiveState> {
    match state {
        viewer::ViewerTabState::Pending => Some(ViewerActiveState::Pending { tab_id }),
        viewer::ViewerTabState::Ready => None,
        viewer::ViewerTabState::Broken { failure } => Some(ViewerActiveState::Broken {
            tab_id,
            failure: failure.clone().into(),
        }),
        viewer::ViewerTabState::Error { failure } => Some(ViewerActiveState::Error {
            tab_id,
            failure: failure.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_render_states_project_without_reason_text_classification() {
        let tab_id = gtl_models::viewer::ViewerTabId::try_new(7).unwrap();

        assert_eq!(
            to_tab_state(&viewer::ViewerTabState::Pending),
            ViewerTabState::Pending
        );
        assert_eq!(
            non_ready_active_state(tab_id, &viewer::ViewerTabState::Pending),
            Some(ViewerActiveState::Pending { tab_id })
        );
        assert_eq!(
            to_tab_state(&viewer::ViewerTabState::Ready),
            ViewerTabState::Ready
        );
        assert_eq!(
            non_ready_active_state(tab_id, &viewer::ViewerTabState::Ready),
            None
        );
    }

    #[test]
    fn failure_states_project_their_typed_reason() {
        let tab_id = gtl_models::viewer::ViewerTabId::try_new(7).unwrap();
        let broken = viewer::ViewerTabState::Broken {
            failure: gtl_models::failure::ViewerFailure::SourceUnavailable,
        };
        let failed = viewer::ViewerTabState::Error {
            failure: gtl_models::failure::ViewerFailure::RenderFailed.into(),
        };

        assert_eq!(
            non_ready_active_state(tab_id, &broken),
            Some(ViewerActiveState::Broken {
                tab_id,
                failure: gtl_models::failure::ViewerFailure::SourceUnavailable.into(),
            })
        );
        assert_eq!(
            non_ready_active_state(tab_id, &failed),
            Some(ViewerActiveState::Error {
                tab_id,
                failure: gtl_models::failure::ViewerFailure::RenderFailed.into(),
            })
        );
    }
}
