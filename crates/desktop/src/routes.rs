mod history;
mod live_views;
mod open_diff_file;
mod parse;
mod render;

use std::collections::VecDeque;

use application::{
    history::{get_recent_render, list_recent_renders},
    ports::UserSettingsStore,
    viewer::{
        RenderOptions, Theme, ViewerHistoryEntry, ViewerSettings, ViewerTabId, ViewerTabKind,
        ViewerTabState,
    },
};
use contracts::recipes::{Recipe, RecipeBatchKind, RecipeOp, RecipeSource, RecipeTarget};
use history::to_viewer_entry;
pub(crate) use parse::{ResumeNonce, Route, SettingChange, parse};
use tauri::http::{Request, Response, StatusCode};

use crate::{
    presentation::ViewerApp,
    recipes::{OpenRecipeOutcome, RecipeError},
    session::PendingRecipesError,
};

const HTML_CONTENT_TYPE: &str = "text/html; charset=utf-8";
const TEXT_CONTENT_TYPE: &str = "text/plain; charset=utf-8";
const DYNAMIC_CACHE_CONTROL: &str = "no-store";
const RECOVERY_HEADER: &str = "X-GTL-Recovery";
const RECOVERY_RESWAP: &str = "outerHTML";
const LAYOUT_KEY: &str = "layout";
const DENSITY_KEY: &str = "density";
const THEME_KEY: &str = "theme";

#[derive(Debug, Clone, Copy)]
enum ErrorTarget {
    Document,
    View,
    Tabs,
    History,
    Action,
}

#[derive(Debug)]
enum RouteError {
    Internal(String),
    Conflict,
    NotFound,
}

impl From<String> for RouteError {
    fn from(value: String) -> Self {
        Self::Internal(value)
    }
}

impl From<render::RenderError> for RouteError {
    fn from(value: render::RenderError) -> Self {
        match value {
            render::RenderError::State(reason) => Self::Internal(reason),
            render::RenderError::Retry | render::RenderError::Conflict => Self::Conflict,
        }
    }
}

impl From<PendingRecipesError> for RouteError {
    fn from(value: PendingRecipesError) -> Self {
        Self::Internal(value.to_string())
    }
}

impl From<RecipeError> for RouteError {
    fn from(value: RecipeError) -> Self {
        match value {
            RecipeError::Failed(reason) => Self::Internal(reason),
            RecipeError::Stale => Self::Conflict,
        }
    }
}

type RouteResult = Result<Response<Vec<u8>>, RouteError>;

#[expect(
    clippy::needless_pass_by_value,
    reason = "the Tauri protocol callback moves its owned request onto a blocking worker"
)]
pub(crate) fn serve_app(app: &ViewerApp, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let route = match parse(&request) {
        Ok(route) => route,
        Err(status) => return status_response(status),
    };
    let target = error_target(&route);
    match serve_route(app, route) {
        Ok(response) => response,
        Err(error) => error_response(target, &error),
    }
}

fn error_target(route: &Route) -> ErrorTarget {
    match route {
        Route::Document { .. } | Route::Settings(_) => ErrorTarget::Document,
        Route::View { .. } | Route::Refresh { .. } | Route::Activate { .. } => ErrorTarget::View,
        Route::Close { .. }
        | Route::DeleteLiveView { .. }
        | Route::OpenHistory { .. }
        | Route::Pending => ErrorTarget::Tabs,
        Route::History => ErrorTarget::History,
        Route::OpenDiffFile { .. } => ErrorTarget::Action,
    }
}

fn error_response(target: ErrorTarget, error: &RouteError) -> Response<Vec<u8>> {
    match &error {
        RouteError::Internal(reason) => eprintln!("gtl-viewer route failure: {reason}"),
        RouteError::Conflict => eprintln!("gtl-viewer route failure: fragment generation conflict"),
        RouteError::NotFound => {}
    }
    let status = match error {
        RouteError::Conflict => StatusCode::CONFLICT,
        RouteError::NotFound => StatusCode::NOT_FOUND,
        RouteError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let body = match target {
        ErrorTarget::Document => render::error_document(),
        ErrorTarget::View => render::error_view(),
        ErrorTarget::Tabs => render::error_tabs(),
        ErrorTarget::History => render::error_history(),
        ErrorTarget::Action => String::new(),
    };
    let builder = Response::builder()
        .status(status)
        .header("Content-Type", HTML_CONTENT_TYPE)
        .header("Cache-Control", DYNAMIC_CACHE_CONTROL);
    let builder = if matches!(target, ErrorTarget::Document | ErrorTarget::Action) {
        builder
    } else {
        builder
            .header(RECOVERY_HEADER, "true")
            .header("HX-Reswap", RECOVERY_RESWAP)
    };
    builder
        .body(body.into_bytes())
        .expect("static error response builds")
}

fn serve_route(app: &ViewerApp, route: Route) -> RouteResult {
    match route {
        Route::Document { .. } => document(app),
        Route::View { tab, options } => view(app, tab, options),
        Route::Refresh { tab } => refresh(app, tab),
        Route::Close { tab } => close(app, tab),
        Route::DeleteLiveView { tab } => live_views::delete(app, tab),
        Route::Activate { tab } => activate(app, tab),
        Route::History => history(app),
        Route::OpenHistory { render } => open_history(app, render),
        Route::Settings(change) => settings(app, change),
        Route::Pending => pending(app),
        Route::OpenDiffFile {
            tab,
            diff_file_path,
        } => open_diff_file::serve(app, tab, diff_file_path),
    }
}

fn document(app: &ViewerApp) -> RouteResult {
    let transient = restore_live_views(app)?;
    let settings = load_settings(app);
    let history = load_history(app)?;
    render::document(app.renderer, &app.session, transient, history, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn view(app: &ViewerApp, tab: ViewerTabId, options: RenderOptions) -> RouteResult {
    {
        let mut session = app.session.lock().map_err(|error| error.to_string())?;
        if !session.activate(tab) {
            return Ok(status_response(StatusCode::NOT_FOUND));
        }
    }
    persist_setting(app, LAYOUT_KEY, options.layout().to_string())?;
    persist_setting(app, DENSITY_KEY, options.density().to_string())?;
    let transient = ensure_active_view(app)?;
    let theme = load_settings(app).theme();
    let settings = ViewerSettings::new(options, theme);
    render::view_with_tabs(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn refresh(app: &ViewerApp, tab: ViewerTabId) -> RouteResult {
    if !tab_exists(app, tab)? {
        return Ok(status_response(StatusCode::NOT_FOUND));
    }
    let result = app.refresh_recipe(tab)?;
    let transient = result.view.map(|view| render::VersionedView {
        ticket: result.ticket,
        view,
    });
    let settings = load_settings(app);
    render::view_with_tabs(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn close(app: &ViewerApp, tab: ViewerTabId) -> RouteResult {
    let closed = app
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .close(tab);
    if !closed {
        return Ok(status_response(StatusCode::NOT_FOUND));
    }
    let transient = ensure_active_view(app)?;
    let settings = load_settings(app);
    render::tabs_with_view(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn activate(app: &ViewerApp, tab: ViewerTabId) -> RouteResult {
    let activated = app
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .activate(tab);
    if !activated {
        return Ok(status_response(StatusCode::NOT_FOUND));
    }
    let transient = ensure_active_view(app)?;
    let settings = load_settings(app);
    render::view_with_tabs(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn history(app: &ViewerApp) -> RouteResult {
    let entries = load_history(app)?;
    Ok(html_response(render::history(app.renderer, &entries)))
}

fn open_history(app: &ViewerApp, id: application::viewer::RenderHistoryId) -> RouteResult {
    let Some(entry) =
        get_recent_render::execute(get_recent_render::GetRecentRender { id }, &app.app_state)
            .map_err(|error| format!("{error:#}"))?
            .entry
    else {
        return Ok(status_response(StatusCode::NOT_FOUND));
    };
    let opened = app.open_recipe(
        &entry.recipe,
        format!("history-{id}"),
        ViewerTabKind::Snapshot,
    )?;
    let transient = match opened {
        OpenRecipeOutcome::Opened(opened) => {
            debug_assert_eq!(opened.tab_id, opened.ticket.tab_id);
            opened.view.map(|view| render::VersionedView {
                ticket: opened.ticket,
                view,
            })
        }
        OpenRecipeOutcome::Skipped { .. } => None,
    };
    let settings = load_settings(app);
    render::tabs_with_view(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn settings(app: &ViewerApp, change: SettingChange) -> RouteResult {
    let (key, value) = match change {
        SettingChange::Layout(value) => (LAYOUT_KEY, value.to_string()),
        SettingChange::Density(value) => (DENSITY_KEY, value.to_string()),
        SettingChange::Theme(value) => (THEME_KEY, value.to_string()),
    };
    persist_setting(app, key, value)?;
    Ok(status_response(StatusCode::NO_CONTENT))
}

fn pending(app: &ViewerApp) -> RouteResult {
    app.pending().with_consumer(|| pending_transaction(app))?
}

fn pending_transaction(app: &ViewerApp) -> RouteResult {
    let batches = app.pending().try_drain()?;
    match process_pending(batches, |recipe, batch_id, kind| {
        app.open_recipe(recipe, batch_id.into(), viewer_tab_kind(kind))
            .map(|outcome| match outcome {
                OpenRecipeOutcome::Opened(opened) => PendingRecipeOutcome::Opened(opened),
                OpenRecipeOutcome::Skipped { label } => PendingRecipeOutcome::Skipped(label),
            })
    }) {
        Ok(processed) => {
            let PendingProcess {
                latest_opened,
                skipped_labels,
            } = processed;
            let transient = latest_opened.and_then(|opened| {
                debug_assert_eq!(opened.tab_id, opened.ticket.tab_id);
                opened.view.map(|view| render::VersionedView {
                    ticket: opened.ticket,
                    view,
                })
            });
            let settings = load_settings(app);
            let html = if skipped_labels.is_empty() {
                render::tabs_with_view(app.renderer, &app.session, transient, settings)
            } else {
                render::tabs_with_view_after_snapshot_skips(
                    app.renderer,
                    &app.session,
                    transient,
                    settings,
                    &skipped_labels,
                )
            }?;
            Ok(html_response(html))
        }
        Err(failure) => {
            app.pending().prepend(failure.remainder)?;
            Err(RouteError::from(failure.reason))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingRecipeOutcome<T> {
    Opened(T),
    Skipped(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingProcess<T> {
    latest_opened: Option<T>,
    skipped_labels: Vec<String>,
}

#[derive(Debug)]
struct PendingFailure<E> {
    reason: E,
    remainder: Vec<contracts::recipes::OpenRecipes>,
}

fn process_pending<T, E>(
    batches: Vec<contracts::recipes::OpenRecipes>,
    mut open: impl FnMut(&Recipe, &str, RecipeBatchKind) -> Result<PendingRecipeOutcome<T>, E>,
) -> Result<PendingProcess<T>, PendingFailure<E>> {
    let mut batches = VecDeque::from(batches);
    let mut processed = PendingProcess {
        latest_opened: None,
        skipped_labels: Vec::new(),
    };
    while let Some(mut batch) = batches.pop_front() {
        while !batch.recipes.is_empty() {
            let recipe = batch.recipes.remove(0);
            match open(&recipe, &batch.batch_id, batch.kind) {
                Ok(PendingRecipeOutcome::Opened(value)) => {
                    processed.latest_opened = Some(value);
                }
                Ok(PendingRecipeOutcome::Skipped(label)) => {
                    processed.skipped_labels.push(label);
                }
                Err(reason) => {
                    batch.recipes.insert(0, recipe);
                    let mut remainder = vec![batch];
                    remainder.extend(batches);
                    return Err(PendingFailure { reason, remainder });
                }
            }
        }
    }
    Ok(processed)
}

const fn viewer_tab_kind(kind: RecipeBatchKind) -> ViewerTabKind {
    match kind {
        RecipeBatchKind::Snapshot => ViewerTabKind::Snapshot,
        RecipeBatchKind::Live => ViewerTabKind::Live,
    }
}

fn restore_live_views(app: &ViewerApp) -> Result<Option<render::VersionedView>, RouteError> {
    let mut transient = None;
    let owner = app.restoration.run_once(|| {
        let records = application::live_views::list::execute(
            application::live_views::list::ListLiveViews,
            &app.app_state,
        )
        .map_err(|error| format!("{error:#}"))?
        .views;
        let mut newest = None;
        {
            let mut session = app.session.lock().map_err(|error| error.to_string())?;
            for record in records {
                if record.source_kind != "LocalRepo" {
                    continue;
                }
                let recipe = Recipe {
                    source: RecipeSource::LocalRepo(record.source_value.into()),
                    op: RecipeOp::Diff {
                        target: RecipeTarget::Unpushed { pinned: None },
                    },
                    name: Some(record.display_name),
                };
                newest = Some(session.open(recipe, "restored-live".into(), ViewerTabKind::Live));
            }
        }
        if let Some(tab) = newest {
            let result = app.refresh_recipe(tab).map_err(|error| error.to_string())?;
            transient = result.view.map(|view| render::VersionedView {
                ticket: result.ticket,
                view,
            });
        }
        Ok(())
    })?;
    if owner {
        Ok(transient)
    } else {
        ensure_active_view(app)
    }
}

fn ensure_active_view(app: &ViewerApp) -> Result<Option<render::VersionedView>, RouteError> {
    let refresh = {
        let mut session = app.session.lock().map_err(|error| error.to_string())?;
        let Some(id) = session.active() else {
            return Ok(None);
        };
        let state = session.tab(id).map(|tab| tab.tab.state().clone());
        let needs_compute = match state {
            Some(ViewerTabState::Ready) => session.cached_view_snapshot(id).is_none(),
            Some(ViewerTabState::Error { reason }) => reason == "render pending",
            _ => false,
        };
        needs_compute.then_some(id)
    };
    let Some(id) = refresh else {
        return Ok(None);
    };
    let result = app.refresh_recipe(id)?;
    Ok(result.view.map(|view| render::VersionedView {
        ticket: result.ticket,
        view,
    }))
}

fn tab_exists(app: &ViewerApp, id: ViewerTabId) -> Result<bool, String> {
    Ok(app
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .tab(id)
        .is_some())
}

fn load_history(app: &ViewerApp) -> Result<Vec<ViewerHistoryEntry>, String> {
    list_recent_renders::execute(list_recent_renders::ListRecentRenders, &app.app_state)
        .map(|response| response.entries.into_iter().map(to_viewer_entry).collect())
        .map_err(|error| format!("{error:#}"))
}

fn load_settings(app: &ViewerApp) -> ViewerSettings {
    let settings = app.user_settings.load();
    let theme = settings
        .theme()
        .and_then(|value| value.parse().ok())
        .unwrap_or(Theme::Dark);
    ViewerSettings::new(settings.viewer_render_options(), theme)
}

fn persist_setting(app: &ViewerApp, key: &str, value_new: String) -> Result<(), String> {
    application::settings::set_key::execute(
        application::settings::set_key::SetSettingKey {
            key: key.into(),
            value_new,
        },
        &app.user_settings,
    )
    .map(|_| ())
    .map_err(|error| format!("{error:#}"))
}

fn html_response(body: String) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", HTML_CONTENT_TYPE)
        .header("Cache-Control", DYNAMIC_CACHE_CONTROL)
        .body(body.into_bytes())
        .expect("static HTML response builds")
}

fn status_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", TEXT_CONTENT_TYPE)
        .header("Cache-Control", DYNAMIC_CACHE_CONTROL)
        .body(Vec::new())
        .expect("static status response builds")
}

#[cfg(test)]
mod tests;
