mod document;
mod history;
mod live_views;
mod open_diff_file;
mod parse;
mod pending;
mod response;
mod settings;
mod tabs;
mod view_loading;
mod view_snapshot;

pub(crate) use parse::{ResumeNonce, Route, SettingChange, parse};
use response::{ErrorTarget, RouteOutput, RouteResult};
use tauri::http::{Request, Response};

use crate::presentation::ViewerApp;

#[expect(
    clippy::needless_pass_by_value,
    reason = "the Tauri protocol callback moves its owned request onto a blocking worker"
)]
pub(crate) fn serve_app(app: &ViewerApp, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let route = match parse(&request) {
        Ok(route) => route,
        Err(status) => return response::into_response(RouteOutput::Empty(status)),
    };
    let target = error_target(&route);
    match serve_route(app, route) {
        Ok(output) => response::into_response(output),
        Err(error) => response::error_response(target, &error),
    }
}

#[cfg(feature = "dioxus-poc")]
pub(crate) fn serve_unmaterialized_view(app: &ViewerApp) -> Response<Vec<u8>> {
    match view_loading::ready_unmaterialized(app) {
        Ok(output) => response::into_response(output),
        Err(error) => response::error_response(ErrorTarget::View, &error),
    }
}

fn error_target(route: &Route) -> ErrorTarget {
    match route {
        Route::Document { .. } | Route::Settings(_) => ErrorTarget::Document,
        Route::View { .. }
        | Route::CommitPatch { .. }
        | Route::Refresh { .. }
        | Route::Activate { .. }
        | Route::Ready => ErrorTarget::View,
        Route::Close { .. }
        | Route::DeleteLiveView { .. }
        | Route::OpenHistory { .. }
        | Route::Pending => ErrorTarget::Tabs,
        Route::History { .. } => ErrorTarget::History,
        Route::OpenDiffFile { .. } | Route::LoadNext { .. } => ErrorTarget::Action,
    }
}

fn serve_route(app: &ViewerApp, route: Route) -> RouteResult {
    match route {
        Route::Document { .. } => document::serve(app),
        Route::View { tab, options } => tabs::view(app, tab, options),
        Route::CommitPatch { tab, sha, options } => tabs::commit_patch(app, tab, &sha, options),
        Route::Refresh { tab } => tabs::refresh(app, tab),
        Route::Close { tab } => tabs::close(app, tab),
        Route::DeleteLiveView { tab } => live_views::delete(app, tab),
        Route::Activate { tab } => tabs::activate(app, tab),
        Route::History { cursor } => history::list(app, cursor),
        Route::OpenHistory { render } => history::open(app, render),
        Route::Settings(change) => settings::serve(app, change),
        Route::Pending => pending::serve(app),
        Route::Ready => view_loading::ready(app),
        Route::LoadNext { load } => view_loading::load_next(app, load),
        Route::OpenDiffFile {
            tab,
            diff_file_path,
        } => open_diff_file::serve(app, tab, diff_file_path),
    }
}
