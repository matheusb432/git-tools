mod history;
mod parse;
mod render;
mod restoration;

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use application::{
    diffs::{
        compute_diff::ComputeDiff, compute_merge_diff::ComputeMergeDiff,
        compute_squash_preview::ComputeSquashPreview,
    },
    history::{
        list_recent::{GetRecentRender, ListRecentRenders},
        record_render::RecordRender,
    },
    live_views::{list::ListLiveViews, probe::ProbeSource},
    settings::{get::GetSetting, set::SetSetting},
};
use cqrsy::{Handle, Sender};
use domain::viewer::{
    DiffDensity, DiffLayout, RenderOptions, Theme, ViewerHistoryEntry, ViewerSettings, ViewerTabId,
    ViewerTabKind, ViewerTabState,
};
use gtl_recipe::{Recipe, RecipeBatchKind, RecipeOp, RecipeSource, RecipeTarget};
use history::to_viewer_entry;
pub(crate) use parse::{ResumeNonce, Route, SettingChange, parse};
use tauri::http::{Request, Response, StatusCode};

use crate::{
    recipes::{RecipeError, open_recipe, refresh_recipe_versioned},
    render::MaudViewerRenderer,
    session::{PendingRecipes, PendingRecipesError, ViewerSession},
};

const HTML_CONTENT_TYPE: &str = "text/html; charset=utf-8";
const TEXT_CONTENT_TYPE: &str = "text/plain; charset=utf-8";
const DYNAMIC_CACHE_CONTROL: &str = "no-store";
const LAYOUT_KEY: &str = "layout";
const DENSITY_KEY: &str = "density";
const THEME_KEY: &str = "theme";

#[derive(Debug, Clone, Copy)]
enum ErrorTarget {
    Document,
    View,
    Tabs,
    History,
}

#[derive(Debug)]
enum RouteError {
    Internal(String),
    Conflict,
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

pub(crate) trait RouteMediator:
    Sender<ComputeDiff>
    + Sender<ComputeMergeDiff>
    + Sender<ComputeSquashPreview>
    + Sender<ProbeSource>
    + Sender<RecordRender>
    + Sender<ListLiveViews>
    + Sender<ListRecentRenders>
    + Sender<GetRecentRender>
    + Sender<GetSetting>
    + Sender<SetSetting>
    + Handle
    + Clone
    + Send
    + Sync
    + 'static
{
}

impl<T> RouteMediator for T where
    T: Sender<ComputeDiff>
        + Sender<ComputeMergeDiff>
        + Sender<ComputeSquashPreview>
        + Sender<ProbeSource>
        + Sender<RecordRender>
        + Sender<ListLiveViews>
        + Sender<ListRecentRenders>
        + Sender<GetRecentRender>
        + Sender<GetSetting>
        + Sender<SetSetting>
        + Handle
        + Clone
        + Send
        + Sync
        + 'static
{
}

#[derive(Clone)]
pub(crate) struct ViewerApp<M> {
    mediator: M,
    session: Arc<Mutex<ViewerSession>>,
    pending: Arc<PendingRecipes>,
    renderer: MaudViewerRenderer,
    data_root: Arc<PathBuf>,
    restoration: Arc<restoration::RestorationGate>,
}

impl<M> ViewerApp<M> {
    pub(crate) fn new(mediator: M, data_root: PathBuf, max_cache_weight: usize) -> Self {
        Self {
            mediator,
            session: Arc::new(Mutex::new(ViewerSession::new(max_cache_weight))),
            pending: Arc::new(PendingRecipes::default()),
            renderer: MaudViewerRenderer,
            data_root: Arc::new(data_root),
            restoration: Arc::new(restoration::RestorationGate::default()),
        }
    }

    pub(crate) fn pending(&self) -> &PendingRecipes {
        &self.pending
    }

    #[cfg(test)]
    fn active_tab(&self) -> Option<ViewerTabId> {
        self.session.lock().expect("session lock").active()
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "the Tauri protocol callback moves its owned request onto a blocking worker"
)]
pub(crate) fn serve_app<M: RouteMediator>(
    app: &ViewerApp<M>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
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
        Route::Close { .. } | Route::OpenHistory { .. } | Route::Pending => ErrorTarget::Tabs,
        Route::History => ErrorTarget::History,
    }
}

fn error_response(target: ErrorTarget, error: &RouteError) -> Response<Vec<u8>> {
    match &error {
        RouteError::Internal(reason) => eprintln!("gtl-viewer route failure: {reason}"),
        RouteError::Conflict => eprintln!("gtl-viewer route failure: fragment generation conflict"),
    }
    let status = if matches!(error, RouteError::Conflict) {
        StatusCode::CONFLICT
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    let body = match target {
        ErrorTarget::Document => render::error_document(),
        ErrorTarget::View => render::error_view(),
        ErrorTarget::Tabs => render::error_tabs(),
        ErrorTarget::History => render::error_history(),
    };
    Response::builder()
        .status(status)
        .header("Content-Type", HTML_CONTENT_TYPE)
        .header("Cache-Control", DYNAMIC_CACHE_CONTROL)
        .body(body.into_bytes())
        .expect("static error response builds")
}

fn serve_route<M: RouteMediator>(app: &ViewerApp<M>, route: Route) -> RouteResult {
    match route {
        Route::Document { .. } => document(app),
        Route::View { tab, options } => view(app, tab, options),
        Route::Refresh { tab } => refresh(app, tab),
        Route::Close { tab } => close(app, tab),
        Route::Activate { tab } => activate(app, tab),
        Route::History => history(app),
        Route::OpenHistory { render } => open_history(app, render),
        Route::Settings(change) => settings(app, change),
        Route::Pending => pending(app),
    }
}

fn document<M: RouteMediator>(app: &ViewerApp<M>) -> RouteResult {
    let transient = restore_live_views(app)?;
    let settings = load_settings(app)?;
    let history = load_history(app)?;
    render::document(app.renderer, &app.session, transient, history, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn view<M: RouteMediator>(
    app: &ViewerApp<M>,
    tab: ViewerTabId,
    options: RenderOptions,
) -> RouteResult {
    {
        let mut session = app.session.lock().map_err(|error| error.to_string())?;
        if !session.activate(tab) {
            return Ok(status_response(StatusCode::NOT_FOUND));
        }
    }
    persist_setting(app, LAYOUT_KEY, options.layout().to_string())?;
    persist_setting(app, DENSITY_KEY, options.density().to_string())?;
    let transient = ensure_active_view(app)?;
    let theme = load_theme(app)?;
    let settings = ViewerSettings::new(options, theme);
    render::view_with_tabs(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn refresh<M: RouteMediator>(app: &ViewerApp<M>, tab: ViewerTabId) -> RouteResult {
    if !tab_exists(app, tab)? {
        return Ok(status_response(StatusCode::NOT_FOUND));
    }
    let result = refresh_recipe_versioned(&app.mediator, &app.session, &app.data_root, tab)?;
    let transient = result.view.map(|view| render::VersionedView {
        ticket: result.ticket,
        view,
    });
    let settings = load_settings(app)?;
    render::view_with_tabs(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn close<M: RouteMediator>(app: &ViewerApp<M>, tab: ViewerTabId) -> RouteResult {
    let closed = app
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .close(tab);
    if !closed {
        return Ok(status_response(StatusCode::NOT_FOUND));
    }
    let transient = ensure_active_view(app)?;
    let settings = load_settings(app)?;
    render::tabs_with_view(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn activate<M: RouteMediator>(app: &ViewerApp<M>, tab: ViewerTabId) -> RouteResult {
    let activated = app
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .activate(tab);
    if !activated {
        return Ok(status_response(StatusCode::NOT_FOUND));
    }
    let transient = ensure_active_view(app)?;
    let settings = load_settings(app)?;
    render::view_with_tabs(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn history<M: RouteMediator>(app: &ViewerApp<M>) -> RouteResult {
    let entries = load_history(app)?;
    Ok(html_response(render::history(app.renderer, &entries)))
}

fn open_history<M: RouteMediator>(
    app: &ViewerApp<M>,
    id: domain::viewer::RenderHistoryId,
) -> RouteResult {
    let Some(entry) = app
        .mediator
        .send_now(GetRecentRender {
            data_root: (*app.data_root).clone(),
            id,
        })
        .map_err(|error| format!("{error:#}"))?
        .entry
    else {
        return Ok(status_response(StatusCode::NOT_FOUND));
    };
    let recipe: Recipe = serde_json::from_str(&entry.recipe_json)
        .map_err(|error| format!("invalid saved recipe for history {id}: {error}"))?;
    let opened = open_recipe(
        &app.mediator,
        &app.session,
        &app.data_root,
        &recipe,
        format!("history-{id}"),
        ViewerTabKind::Snapshot,
    )?;
    let transient = Some(opened).and_then(|opened| {
        debug_assert_eq!(opened.tab_id, opened.ticket.tab_id);
        opened.view.map(|view| render::VersionedView {
            ticket: opened.ticket,
            view,
        })
    });
    let settings = load_settings(app)?;
    render::tabs_with_view(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}

fn settings<M: RouteMediator>(app: &ViewerApp<M>, change: SettingChange) -> RouteResult {
    let (key, value) = match change {
        SettingChange::Layout(value) => (LAYOUT_KEY, value.to_string()),
        SettingChange::Density(value) => (DENSITY_KEY, value.to_string()),
        SettingChange::Theme(value) => (THEME_KEY, value.to_string()),
    };
    persist_setting(app, key, value)?;
    Ok(status_response(StatusCode::NO_CONTENT))
}

fn pending<M: RouteMediator>(app: &ViewerApp<M>) -> RouteResult {
    app.pending.with_consumer(|| pending_transaction(app))?
}

fn pending_transaction<M: RouteMediator>(app: &ViewerApp<M>) -> RouteResult {
    let batches = app.pending.try_drain()?;
    match process_pending(batches, |recipe, batch_id, kind| {
        open_recipe(
            &app.mediator,
            &app.session,
            &app.data_root,
            recipe,
            batch_id.into(),
            viewer_tab_kind(kind),
        )
    }) {
        Ok(latest) => {
            let transient = latest.and_then(|opened| {
                debug_assert_eq!(opened.tab_id, opened.ticket.tab_id);
                opened.view.map(|view| render::VersionedView {
                    ticket: opened.ticket,
                    view,
                })
            });
            let settings = load_settings(app)?;
            render::tabs_with_view(app.renderer, &app.session, transient, settings)
                .map(html_response)
                .map_err(Into::into)
        }
        Err(failure) => {
            app.pending.prepend(failure.remainder)?;
            Err(RouteError::from(failure.reason))
        }
    }
}

#[derive(Debug)]
struct PendingFailure<E> {
    reason: E,
    remainder: Vec<gtl_recipe::OpenRecipes>,
}

fn process_pending<T, E>(
    batches: Vec<gtl_recipe::OpenRecipes>,
    mut open: impl FnMut(&Recipe, &str, RecipeBatchKind) -> Result<T, E>,
) -> Result<Option<T>, PendingFailure<E>> {
    let mut batches = VecDeque::from(batches);
    let mut latest = None;
    while let Some(mut batch) = batches.pop_front() {
        while !batch.recipes.is_empty() {
            let recipe = batch.recipes.remove(0);
            match open(&recipe, &batch.batch_id, batch.kind) {
                Ok(value) => latest = Some(value),
                Err(reason) => {
                    batch.recipes.insert(0, recipe);
                    let mut remainder = vec![batch];
                    remainder.extend(batches);
                    return Err(PendingFailure { reason, remainder });
                }
            }
        }
    }
    Ok(latest)
}

const fn viewer_tab_kind(kind: RecipeBatchKind) -> ViewerTabKind {
    match kind {
        RecipeBatchKind::Snapshot => ViewerTabKind::Snapshot,
        RecipeBatchKind::Live => ViewerTabKind::Live,
    }
}

fn restore_live_views<M: RouteMediator>(
    app: &ViewerApp<M>,
) -> Result<Option<render::VersionedView>, RouteError> {
    let mut transient = None;
    let owner = app.restoration.run_once(|| {
        let records = app
            .mediator
            .send_now(ListLiveViews {
                data_root: (*app.data_root).clone(),
            })
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
                        target: RecipeTarget::Unpushed,
                    },
                    name: Some(record.display_name),
                };
                newest = Some(session.open(recipe, "restored-live".into(), ViewerTabKind::Live));
            }
        }
        if let Some(tab) = newest {
            let result = refresh_recipe_versioned(&app.mediator, &app.session, &app.data_root, tab)
                .map_err(|error| error.to_string())?;
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

fn ensure_active_view<M: RouteMediator>(
    app: &ViewerApp<M>,
) -> Result<Option<render::VersionedView>, RouteError> {
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
    let result = refresh_recipe_versioned(&app.mediator, &app.session, &app.data_root, id)?;
    Ok(result.view.map(|view| render::VersionedView {
        ticket: result.ticket,
        view,
    }))
}

fn tab_exists<M>(app: &ViewerApp<M>, id: ViewerTabId) -> Result<bool, String> {
    Ok(app
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .tab(id)
        .is_some())
}

fn load_history<M: RouteMediator>(app: &ViewerApp<M>) -> Result<Vec<ViewerHistoryEntry>, String> {
    app.mediator
        .send_now(ListRecentRenders {
            data_root: (*app.data_root).clone(),
        })
        .map(|response| response.entries.into_iter().map(to_viewer_entry).collect())
        .map_err(|error| format!("{error:#}"))
}

fn load_settings<M: RouteMediator>(app: &ViewerApp<M>) -> Result<ViewerSettings, String> {
    let layout = load_setting(app, LAYOUT_KEY)?
        .map_or(Ok(DiffLayout::Unified), |value| value.parse())
        .map_err(|error| error.to_string())?;
    let density = load_setting(app, DENSITY_KEY)?
        .map_or(Ok(DiffDensity::Compact), |value| value.parse())
        .map_err(|error| error.to_string())?;
    let theme = load_theme(app)?;
    Ok(ViewerSettings::new(
        RenderOptions::new(layout, density),
        theme,
    ))
}

fn load_theme<M: RouteMediator>(app: &ViewerApp<M>) -> Result<Theme, String> {
    load_setting(app, THEME_KEY)?
        .map_or(Ok(Theme::Dark), |value| value.parse())
        .map_err(|error| error.to_string())
}

fn load_setting<M: RouteMediator>(app: &ViewerApp<M>, key: &str) -> Result<Option<String>, String> {
    app.mediator
        .send_now(GetSetting {
            data_root: (*app.data_root).clone(),
            key: key.into(),
        })
        .map(|response| response.value)
        .map_err(|error| format!("{error:#}"))
}

fn persist_setting<M: RouteMediator>(
    app: &ViewerApp<M>,
    key: &str,
    value: String,
) -> Result<(), String> {
    app.mediator
        .send_now(SetSetting {
            data_root: (*app.data_root).clone(),
            key: key.into(),
            value,
        })
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
