use std::path::PathBuf;

use gtl_application::{
    diffs::open_diff_file_in_configured_editor::{
        self, OpenDiffFileInConfiguredEditor, OpenDiffFileInConfiguredEditorError,
    },
    live_views,
};
use gtl_models::viewer::{ViewerTabId, ViewerTabState};
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
    let tab_id = tab_id(request.tab_id)?;
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
    let tab_id = tab_id(request.tab_id)?;
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
    let tab_id = tab_id(request.tab_id)?;
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
    let tab_id = tab_id(request.tab_id)?;
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
        live_views::remove::execute(
            live_views::remove::RemoveLiveView {
                source_kind: source.kind().into(),
                source_value: source.value(),
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
    let SelectViewerCommit {
        tab_id: raw_tab_id,
        sha,
    } = request;
    let tab_id = tab_id(raw_tab_id)?;
    let sha = validated_sha(&sha)?;
    app.select_commit(tab_id, &sha)
        .map_err(map_select_commit_error)?;
    shell::load(app, None)
}

pub(super) fn clear_commit_selection(
    app: &ViewerApp,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let tab_id = tab_id(request.tab_id)?;
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
    let (key, value_new) = preference_pair(request);
    settings::set_root_key(app, key.into(), value_new)?;
    shell::load(app, None)
}

pub(super) fn open_diff_file(
    app: &ViewerApp,
    request: OpenViewerDiffFile,
) -> Result<(), ViewerApiError> {
    if request.path.is_empty() || request.path.len() > DIFF_FILE_PATH_BYTES_MAX {
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
        diff_file_path: PathBuf::from(request.path),
    };
    open_diff_file_in_configured_editor::execute(
        command,
        &view,
        &app.file_system,
        &app.configured_editor,
    )
    .map_err(map_open_diff_file_error)
}

fn tab_id(raw: u64) -> Result<ViewerTabId, ViewerApiError> {
    ViewerTabId::try_new(raw).map_err(|_| ViewerApiError::InvalidRequest)
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

fn validated_sha(raw: &str) -> Result<String, ViewerApiError> {
    if raw.len() != 40 || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ViewerApiError::InvalidRequest);
    }
    Ok(raw.to_ascii_lowercase())
}

fn preference_pair(preference: SetViewerPreference) -> (&'static str, String) {
    match preference {
        SetViewerPreference::Layout(layout) => (
            "layout",
            match layout {
                gtl_wire::viewer::ViewerDiffLayout::Unified => "unified",
                gtl_wire::viewer::ViewerDiffLayout::Split => "split",
            }
            .into(),
        ),
        SetViewerPreference::Density(density) => (
            "density",
            match density {
                gtl_wire::viewer::ViewerDiffDensity::Compact => "compact",
                gtl_wire::viewer::ViewerDiffDensity::Full => "full",
            }
            .into(),
        ),
        SetViewerPreference::Theme(theme) => ("theme", shell::from_theme(theme).to_string()),
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
        | OpenDiffFileInConfiguredEditorError::DiffFilePathInvalid
        | OpenDiffFileInConfiguredEditorError::DiffFileUnavailable
        | OpenDiffFileInConfiguredEditorError::DiffFileOutsideRepository => {
            not_found(ViewerResource::DiffFile)
        }
        OpenDiffFileInConfiguredEditorError::FileSystem(_)
        | OpenDiffFileInConfiguredEditorError::ConfiguredEditorDiscovery(_)
        | OpenDiffFileInConfiguredEditorError::ConfiguredEditorCommand(_)
        | OpenDiffFileInConfiguredEditorError::ConfiguredEditorLaunch(_) => unavailable(
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
    fn commit_identity_is_full_hex_and_normalized() {
        let uppercase = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";

        assert_eq!(
            validated_sha(uppercase).expect("valid SHA"),
            uppercase.to_ascii_lowercase()
        );
        assert_eq!(validated_sha("abc"), Err(ViewerApiError::InvalidRequest));
        assert_eq!(
            validated_sha("gggggggggggggggggggggggggggggggggggggggg"),
            Err(ViewerApiError::InvalidRequest)
        );
    }

    #[test]
    fn preferences_map_only_to_supported_root_setting_keys() {
        assert_eq!(
            preference_pair(SetViewerPreference::Layout(ViewerDiffLayout::Split)),
            ("layout", "split".into())
        );
        assert_eq!(
            preference_pair(SetViewerPreference::Theme(ViewerTheme::Glacier)),
            ("theme", "glacier".into())
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
