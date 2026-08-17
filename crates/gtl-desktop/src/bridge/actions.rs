use gtl_application::{
    diffs::open_diff_file_in_configured_editor::{
        self, OpenDiffFileInConfiguredEditor, OpenDiffFileInConfiguredEditorError,
    },
    live_views::remove_live_view,
};
use gtl_models::{
    settings::SettingKeyValue,
    viewer::{DiffDensity, DiffLayout, ViewerTabState},
};
use gtl_wire::viewer::{
    OpenViewerDiffFile, SelectViewerCommit, SetViewerPreference, ViewerApiError, ViewerFeedback,
    ViewerResource, ViewerShell, ViewerTabRequest,
};

use super::{diff, internal, settings, shell, unavailable};
use crate::{
    presentation::ViewerApp,
    recipes::{RecipeError, SelectCommitError},
    session::{BeginCommitSelectionError, CloseOutcome, RENDER_PENDING_REASON},
};

const DIFF_FILE_PATH_BYTES_MAX: usize = 16 * 1024;

pub(super) fn activate_tab(
    app: &ViewerApp,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let tab_id = request.tab_id;
    let mut session = app
        .session
        .lock()
        .map_err(|error| internal("failed to lock viewer session", error))?;
    if !session.activate(tab_id) {
        return Err(not_found(ViewerResource::Tab));
    }
    session.clear_commit_selection(tab_id);
    drop(session);
    refresh_pending_active_tab(app)?;
    shell::load(app, None)
}

pub(super) fn close_tab(
    app: &ViewerApp,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let tab_id = request.tab_id;
    let outcome = app
        .session
        .lock()
        .map_err(|error| internal("failed to lock viewer session", error))?
        .close(tab_id);
    let Some(outcome) = outcome else {
        return Err(not_found(ViewerResource::Tab));
    };
    if outcome == CloseOutcome::ActiveChanged {
        refresh_pending_active_tab(app)?;
    }
    shell::load(app, Some(ViewerFeedback::TabClosed))
}

pub(super) fn refresh_tab(
    app: &ViewerApp,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let tab_id = request.tab_id;
    let exists = app
        .session
        .lock()
        .map_err(|error| internal("failed to lock viewer session", error))?
        .tab(tab_id)
        .is_some();
    if !exists {
        return Err(not_found(ViewerResource::Tab));
    }
    app.refresh_recipe(tab_id).map_err(map_recipe_error)?;
    shell::load(app, None)
}

pub(super) fn delete_live_tab(
    app: &ViewerApp,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let tab_id = request.tab_id;
    let (source, was_active) = {
        let session = app
            .session
            .lock()
            .map_err(|error| internal("failed to lock viewer session", error))?;
        (
            session
                .live_source(tab_id)
                .ok_or_else(|| not_found(ViewerResource::LiveView))?,
            session.active() == Some(tab_id),
        )
    };
    {
        let connection = app.app_state.connection_lock().map_err(|error| {
            internal("failed to lock saved live views", format_args!("{error:#}"))
        })?;
        remove_live_view::execute(
            remove_live_view::RemoveLiveView {
                source: source.clone(),
            },
            &connection,
        )
        .map_err(|error| internal("failed to delete saved live view", error))?;
    }
    let closed = app
        .session
        .lock()
        .map_err(|error| internal("failed to lock viewer session", error))?
        .close_live_view(tab_id, &source);
    if !closed {
        return Err(ViewerApiError::Conflict);
    }
    if was_active {
        refresh_pending_active_tab(app)?;
    }
    shell::load(app, Some(ViewerFeedback::LiveViewDeleted))
}

pub(super) fn select_commit(
    app: &ViewerApp,
    request: SelectViewerCommit,
) -> Result<ViewerShell, ViewerApiError> {
    let SelectViewerCommit { tab_id, id } = request;
    app.select_commit(tab_id, &id)
        .map_err(map_select_commit_error)?;
    shell::load(app, None)
}

pub(super) fn clear_commit_selection(
    app: &ViewerApp,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let tab_id = request.tab_id;
    let cleared = app
        .session
        .lock()
        .map_err(|error| internal("failed to lock viewer session", error))?
        .clear_commit_selection(tab_id);
    if !cleared {
        return Err(not_found(ViewerResource::Tab));
    }
    shell::load(app, None)
}

pub(super) fn set_preference(
    app: &ViewerApp,
    request: SetViewerPreference,
) -> Result<ViewerShell, ViewerApiError> {
    settings::set_root_key(app, preference_mutation(request))?;
    shell::load(app, None)
}

pub(super) fn open_diff_file(
    app: &ViewerApp,
    request: OpenViewerDiffFile,
) -> Result<(), ViewerApiError> {
    if request.path.as_path().as_os_str().len() > DIFF_FILE_PATH_BYTES_MAX {
        return Err(ViewerApiError::InvalidRequest);
    }
    let options = diff::validated_current_options(app, request.identity)?;
    let view = {
        let mut session = app
            .session
            .lock()
            .map_err(|error| internal("failed to lock viewer session", error))?;
        let snapshot = session
            .active_content_snapshot()
            .ok_or(ViewerApiError::Conflict)?;
        if !shell::identity_matches(request.identity, snapshot.identity(), options) {
            return Err(ViewerApiError::Conflict);
        }
        snapshot.shared_view()
    };
    diff::validate_current_request(app, request.identity, options)?;
    let command = OpenDiffFileInConfiguredEditor {
        diff_file_path: request.path,
    };
    open_diff_file_in_configured_editor::execute(command, &view, &app.file_system, &app.text_editor)
        .map_err(map_open_diff_file_error)
}

fn refresh_pending_active_tab(app: &ViewerApp) -> Result<(), ViewerApiError> {
    let pending = {
        let session = app
            .session
            .lock()
            .map_err(|error| internal("failed to lock viewer session", error))?;
        session.active().filter(|id| {
            matches!(
                session.tab(*id).map(|tab| tab.tab.state()),
                Some(ViewerTabState::Error { reason }) if reason == RENDER_PENDING_REASON
            )
        })
    };
    if let Some(tab_id) = pending {
        app.refresh_recipe(tab_id).map_err(map_recipe_error)?;
    }
    Ok(())
}

fn preference_mutation(preference: SetViewerPreference) -> SettingKeyValue {
    match preference {
        SetViewerPreference::Layout(layout) => SettingKeyValue::Layout(match layout {
            gtl_wire::viewer::ViewerDiffLayout::Unified => DiffLayout::Unified,
            gtl_wire::viewer::ViewerDiffLayout::Split => DiffLayout::Split,
        }),
        SetViewerPreference::Density(density) => SettingKeyValue::Density(match density {
            gtl_wire::viewer::ViewerDiffDensity::Compact => DiffDensity::Compact,
            gtl_wire::viewer::ViewerDiffDensity::Full => DiffDensity::Full,
        }),
        SetViewerPreference::Theme(theme) => SettingKeyValue::Theme(shell::from_theme(theme)),
    }
}

const fn not_found(resource: ViewerResource) -> ViewerApiError {
    ViewerApiError::NotFound { resource }
}

fn map_recipe_error(error: RecipeError) -> ViewerApiError {
    match error {
        RecipeError::Stale => ViewerApiError::Conflict,
        RecipeError::Failed(reason) => internal("viewer recipe action failed", reason),
    }
}

fn map_select_commit_error(error: SelectCommitError) -> ViewerApiError {
    match error {
        SelectCommitError::Reserve(BeginCommitSelectionError::UnknownTab) => {
            not_found(ViewerResource::Tab)
        }
        SelectCommitError::Reserve(BeginCommitSelectionError::UnknownCommit) => {
            not_found(ViewerResource::Commit)
        }
        SelectCommitError::Reserve(
            BeginCommitSelectionError::StaleRange | BeginCommitSelectionError::SelectionPending,
        ) => ViewerApiError::Conflict,
        SelectCommitError::Failed(reason) => internal("commit selection failed", reason),
    }
}

fn map_open_diff_file_error(error: OpenDiffFileInConfiguredEditorError) -> ViewerApiError {
    match error {
        OpenDiffFileInConfiguredEditorError::FileNotInCurrentDiff
        | OpenDiffFileInConfiguredEditorError::DiffFileDeleted
        | OpenDiffFileInConfiguredEditorError::DiffFileUnavailable
        | OpenDiffFileInConfiguredEditorError::DiffFileOutsideRepository => {
            not_found(ViewerResource::DiffFile)
        }
        OpenDiffFileInConfiguredEditorError::FileSystem(_)
        | OpenDiffFileInConfiguredEditorError::ConfiguredEditorDiscovery(_)
        | OpenDiffFileInConfiguredEditorError::ConfiguredEditorCommand(_)
        | OpenDiffFileInConfiguredEditorError::ConfiguredEditorOpen(_) => unavailable(
            ViewerResource::DiffFile,
            "failed to open diff file in the configured editor",
            error,
        ),
    }
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::{ViewerDiffLayout, ViewerTheme};

    use super::*;

    #[test]
    fn preferences_map_to_typed_setting_mutations() {
        assert_eq!(
            preference_mutation(SetViewerPreference::Layout(ViewerDiffLayout::Split)),
            SettingKeyValue::Layout(DiffLayout::Split)
        );
        assert_eq!(
            preference_mutation(SetViewerPreference::Theme(ViewerTheme::Glacier)),
            SettingKeyValue::Theme(gtl_models::viewer::Theme::Glacier)
        );
    }

    #[test]
    fn a_pending_commit_selection_maps_to_a_retryable_conflict() {
        assert_eq!(
            map_select_commit_error(SelectCommitError::Reserve(
                BeginCommitSelectionError::SelectionPending,
            )),
            ViewerApiError::Conflict
        );
    }
}
