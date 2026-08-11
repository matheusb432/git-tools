mod actions;
mod diff;
mod history;
mod settings;
mod shell;

use std::fmt::Display;

use gtl_contracts::viewer::{
    GetViewerHistoryCopy, ListViewerHistory, LoadViewerDiffLines, OpenViewerDiffFile,
    OpenViewerHistory, SelectViewerCommit, SetViewerPreference, ViewerApiError, ViewerDiffLines,
    ViewerHistoryCopyPayload, ViewerHistoryPage, ViewerResource, ViewerShell, ViewerTabRequest,
    ViewerUserSettings,
};
use tauri::State;

use crate::presentation::ViewerApp;

pub(super) fn internal(context: &str, error: impl Display) -> ViewerApiError {
    eprintln!("gtl-viewer: {context}: {error}");
    ViewerApiError::Internal
}

pub(super) fn unavailable(
    resource: ViewerResource,
    context: &str,
    error: impl Display,
) -> ViewerApiError {
    eprintln!("gtl-viewer: {context}: {error}");
    ViewerApiError::Unavailable { resource }
}

async fn run_blocking<T>(
    operation: &'static str,
    task: impl FnOnce() -> Result<T, ViewerApiError> + Send + 'static,
) -> Result<T, ViewerApiError>
where
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| internal(operation, error))?
}

#[tauri::command]
pub(crate) async fn viewer_get_shell(
    app: State<'_, ViewerApp>,
) -> Result<ViewerShell, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("viewer shell worker failed", move || {
        shell::load(&app, None)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_load_diff_lines(
    app: State<'_, ViewerApp>,
    request: LoadViewerDiffLines,
) -> Result<ViewerDiffLines, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("diff lines worker failed", move || {
        diff::load_lines(&app, &request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_activate_tab(
    app: State<'_, ViewerApp>,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("activate tab worker failed", move || {
        actions::activate_tab(&app, request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_close_tab(
    app: State<'_, ViewerApp>,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("close tab worker failed", move || {
        actions::close_tab(&app, request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_refresh_tab(
    app: State<'_, ViewerApp>,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("refresh tab worker failed", move || {
        actions::refresh_tab(&app, request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_delete_live_tab(
    app: State<'_, ViewerApp>,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("delete live tab worker failed", move || {
        actions::delete_live_tab(&app, request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_select_commit(
    app: State<'_, ViewerApp>,
    request: SelectViewerCommit,
) -> Result<ViewerShell, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("select commit worker failed", move || {
        actions::select_commit(&app, request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_clear_commit_selection(
    app: State<'_, ViewerApp>,
    request: ViewerTabRequest,
) -> Result<ViewerShell, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("clear commit selection worker failed", move || {
        actions::clear_commit_selection(&app, request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_set_preference(
    app: State<'_, ViewerApp>,
    request: SetViewerPreference,
) -> Result<ViewerShell, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("set preference worker failed", move || {
        actions::set_preference(&app, request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_list_history(
    app: State<'_, ViewerApp>,
    request: ListViewerHistory,
) -> Result<ViewerHistoryPage, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("list history worker failed", move || {
        history::list(&app, request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_open_history(
    app: State<'_, ViewerApp>,
    request: OpenViewerHistory,
) -> Result<ViewerShell, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("open history worker failed", move || {
        history::open(&app, request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_get_history_copy(
    app: State<'_, ViewerApp>,
    request: GetViewerHistoryCopy,
) -> Result<ViewerHistoryCopyPayload, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("history copy worker failed", move || {
        history::copy(&app, request)
    })
    .await
}

#[tauri::command]
pub(crate) async fn viewer_get_settings(
    app: State<'_, ViewerApp>,
) -> Result<ViewerUserSettings, ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("get settings worker failed", move || settings::get(&app)).await
}

#[tauri::command]
pub(crate) async fn viewer_open_diff_file(
    app: State<'_, ViewerApp>,
    request: OpenViewerDiffFile,
) -> Result<(), ViewerApiError> {
    let app = app.inner().clone();
    run_blocking("open diff file worker failed", move || {
        actions::open_diff_file(&app, request)
    })
    .await
}
