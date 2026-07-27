use std::path::PathBuf;

use application::{
    diffs::open_diff_file_in_configured_editor::{
        self, OpenDiffFileInConfiguredEditor, OpenDiffFileInConfiguredEditorError,
    },
    viewer::ViewerTabId,
};
use tauri::http::StatusCode;

use super::{RouteError, RouteResult, ViewerApp, status_response};

pub(super) fn serve(app: &ViewerApp, tab: ViewerTabId, diff_file_path: PathBuf) -> RouteResult {
    let cached_view = app
        .session
        .lock()
        .map_err(|_| RouteError::Internal("viewer session lock poisoned".into()))?
        .cached_view_snapshot(tab)
        .ok_or(RouteError::NotFound)?;
    let command = OpenDiffFileInConfiguredEditor { diff_file_path };

    match open_diff_file_in_configured_editor::execute(
        command,
        cached_view.view.as_ref(),
        &app.file_system,
        &app.configured_editor,
    ) {
        Ok(()) => Ok(status_response(StatusCode::NO_CONTENT)),
        Err(
            OpenDiffFileInConfiguredEditorError::FileNotInCurrentDiff
            | OpenDiffFileInConfiguredEditorError::DiffFileDeleted
            | OpenDiffFileInConfiguredEditorError::DiffFilePathInvalid
            | OpenDiffFileInConfiguredEditorError::DiffFileUnavailable
            | OpenDiffFileInConfiguredEditorError::DiffFileOutsideRepository,
        ) => Err(RouteError::NotFound),
        Err(error) => Err(RouteError::Internal(error.to_string())),
    }
}
