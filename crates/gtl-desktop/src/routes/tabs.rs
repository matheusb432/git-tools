use std::sync::Mutex;

use gtl_application::viewer::{
    RenderOptions, ViewerHistoryPage, ViewerSettings, ViewerTabId, ViewerTabState,
};
use tauri::http::StatusCode;

use super::{
    response::{RouteOutput, RouteResult},
    settings, view_loading,
    view_snapshot::{self, GENERATION_ATTEMPTS_MAX, RenderError, VersionedView},
};
use crate::{
    materialization::ViewLoadId,
    presentation::ViewerApp,
    render::{MaudViewerRenderer, SwapFeedback},
    session::{CloseOutcome, ViewerSession},
};

pub(super) fn view(app: &ViewerApp, tab: ViewerTabId, options: RenderOptions) -> RouteResult {
    {
        let mut session = app.session.lock().map_err(|error| error.to_string())?;
        if !session.activate(tab) {
            return Ok(RouteOutput::Empty(StatusCode::NOT_FOUND));
        }
        session.clear_commit_selection(tab);
    }
    settings::persist_render_options(app, options)?;
    let transient = view_loading::ensure_active_view(app)?;
    let theme = settings::load(app)?.theme();
    let settings = ViewerSettings::new(options, theme);
    let load_id = view_loading::prepare_materialization(app, settings.options())?;
    render_view_with_tabs(
        app.renderer,
        &app.session,
        transient,
        settings,
        SwapFeedback::None,
        load_id,
    )
    .map(RouteOutput::Html)
    .map_err(Into::into)
}

pub(super) fn commit_patch(
    app: &ViewerApp,
    tab: ViewerTabId,
    sha: &str,
    options: RenderOptions,
) -> RouteResult {
    settings::persist_render_options(app, options)?;
    let retained = app
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .has_ready_commit_selection(tab, sha);
    if !retained {
        app.select_commit(tab, sha)?;
    }
    let theme = settings::load(app)?.theme();
    let settings = ViewerSettings::new(options, theme);
    render_view_with_tabs(
        app.renderer,
        &app.session,
        None,
        settings,
        SwapFeedback::None,
        None,
    )
    .map(RouteOutput::Html)
    .map_err(Into::into)
}

pub(super) fn refresh(app: &ViewerApp, tab: ViewerTabId) -> RouteResult {
    if !tab_exists(app, tab)? {
        return Ok(RouteOutput::Empty(StatusCode::NOT_FOUND));
    }
    app.refresh_recipe(tab)?;
    Ok(RouteOutput::Empty(StatusCode::NO_CONTENT))
}

pub(super) fn close(app: &ViewerApp, tab: ViewerTabId) -> RouteResult {
    let outcome = {
        let mut session = app.session.lock().map_err(|error| error.to_string())?;
        session.close(tab)
    };
    let Some(outcome) = outcome else {
        return Ok(RouteOutput::Empty(StatusCode::NOT_FOUND));
    };
    let settings = settings::load(app)?;
    if outcome == CloseOutcome::ActiveUnchanged {
        return render_tabs_only(app.renderer, &app.session, settings, SwapFeedback::None)
            .map(RouteOutput::Html)
            .map_err(Into::into);
    }
    let transient = view_loading::ensure_active_view(app)?;
    let load_id = view_loading::prepare_materialization(app, settings.options())?;
    render_view_with_tabs(
        app.renderer,
        &app.session,
        transient,
        settings,
        SwapFeedback::TabClosed,
        load_id,
    )
    .map(RouteOutput::Html)
    .map_err(Into::into)
}

fn tab_exists(app: &ViewerApp, id: ViewerTabId) -> Result<bool, String> {
    Ok(app
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .tab(id)
        .is_some())
}

pub(super) fn activate(app: &ViewerApp, tab: ViewerTabId) -> RouteResult {
    let activated = {
        let mut session = app.session.lock().map_err(|error| error.to_string())?;
        let activated = session.activate(tab);
        if activated {
            session.clear_commit_selection(tab);
        }
        activated
    };
    if !activated {
        return Ok(RouteOutput::Empty(StatusCode::NOT_FOUND));
    }
    let transient = view_loading::ensure_active_view(app)?;
    let settings = settings::load(app)?;
    let load_id = view_loading::prepare_materialization(app, settings.options())?;
    render_view_with_tabs(
        app.renderer,
        &app.session,
        transient,
        settings,
        SwapFeedback::None,
        load_id,
    )
    .map(RouteOutput::Html)
    .map_err(Into::into)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "bounded retries clone one validated settings value"
)]
pub(super) fn render_view_with_tabs(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    mut transient: Option<VersionedView>,
    settings: ViewerSettings,
    feedback: SwapFeedback<'_>,
    load_id: Option<ViewLoadId>,
) -> Result<String, RenderError> {
    for _ in 0..GENERATION_ATTEMPTS_MAX {
        let snapshot = match view_snapshot::gather(
            session,
            transient.take(),
            ViewerHistoryPage::default(),
            settings.clone(),
        ) {
            Ok(snapshot) => snapshot,
            Err(RenderError::Retry) => continue,
            Err(error) => return Err(error),
        };
        let can_materialize = snapshot.ticket.is_some()
            && matches!(
                snapshot
                    .document
                    .active_tab()
                    .map(gtl_application::viewer::ViewerTab::state),
                Some(ViewerTabState::Ready)
            );
        let html = match (can_materialize, load_id) {
            (true, Some(load_id)) => renderer.build_materialized_view_with_tabs_feedback(
                &snapshot.document,
                feedback,
                load_id,
            ),
            _ => renderer.build_view_with_tabs_feedback(&snapshot.document, feedback),
        }?;
        if view_snapshot::is_current(session, snapshot.ticket, snapshot.revision)? {
            return Ok(html);
        }
    }
    Err(RenderError::Conflict)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "bounded retries clone one validated settings value"
)]
pub(super) fn render_tabs_with_view(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    mut transient: Option<VersionedView>,
    settings: ViewerSettings,
    feedback: SwapFeedback<'_>,
    load_id: Option<ViewLoadId>,
) -> Result<String, RenderError> {
    for _ in 0..GENERATION_ATTEMPTS_MAX {
        let snapshot = match view_snapshot::gather(
            session,
            transient.take(),
            ViewerHistoryPage::default(),
            settings.clone(),
        ) {
            Ok(snapshot) => snapshot,
            Err(RenderError::Retry) => continue,
            Err(error) => return Err(error),
        };
        let html = match load_id {
            Some(load_id) => {
                renderer.build_materialized_tabs_with_view(&snapshot.document, feedback, load_id)
            }
            None => renderer.build_tabs_with_view(&snapshot.document, feedback),
        }?;
        if view_snapshot::is_current(session, snapshot.ticket, snapshot.revision)? {
            return Ok(html);
        }
    }
    Err(RenderError::Conflict)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "bounded retries clone one validated settings value"
)]
pub(super) fn render_tabs_only(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    settings: ViewerSettings,
    feedback: SwapFeedback<'_>,
) -> Result<String, RenderError> {
    for _ in 0..GENERATION_ATTEMPTS_MAX {
        let snapshot = match view_snapshot::gather(
            session,
            None,
            ViewerHistoryPage::default(),
            settings.clone(),
        ) {
            Ok(snapshot) => snapshot,
            Err(RenderError::Retry) => continue,
            Err(error) => return Err(error),
        };
        let html = renderer.build_tabs_only(&snapshot.document, feedback);
        if view_snapshot::is_current(session, snapshot.ticket, snapshot.revision)? {
            return Ok(html);
        }
    }
    Err(RenderError::Conflict)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use gtl_application::{
        diffs::{Cmd, Foot, View},
        viewer::{RenderOptions, Theme, ViewerSettings, ViewerTabKind},
    };
    use gtl_contracts::recipes::{Recipe, RecipeOp, RecipeSource};

    use super::*;
    use crate::session::{CachedView, ViewerSession};

    fn view(title: &str) -> Arc<View> {
        Arc::new(View {
            exclusions: None,
            repo_name: "repo".into(),
            repo_root: "/repo".into(),
            branch: "feature".into(),
            upstream: "main".into(),
            commits: vec![],
            files: vec![],
            title: title.into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: String::new(),
            foot: Foot {
                cmd: String::new(),
                note: String::new(),
            },
        })
    }

    fn settings() -> ViewerSettings {
        ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark)
    }

    #[test]
    fn stale_transient_followed_by_current_oversize_view_is_bounded_conflict() {
        let recipe = Recipe {
            source: RecipeSource::LocalRepo("/repo".into()),
            op: RecipeOp::MergeDiff {
                base: None,
                pinned: None,
            },
            name: None,
        };
        let mut state = ViewerSession::new(1);
        let id = state
            .open(recipe, "batch".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let old = state.begin_compute(id).expect("old ticket");
        let old_view = view("old transient");
        state.publish_labeled_if_current(old, CachedView::new(Arc::clone(&old_view)), "old".into());
        let current = state.begin_compute(id).expect("current ticket");
        state.publish_labeled_if_current(
            current,
            CachedView::new(view("current oversize")),
            "current".into(),
        );
        let session = Mutex::new(state);

        let error = render_view_with_tabs(
            MaudViewerRenderer,
            &session,
            Some(VersionedView {
                ticket: old,
                view: old_view,
            }),
            settings(),
            SwapFeedback::None,
            None,
        )
        .expect_err("ready oversize view retries to conflict");
        assert!(matches!(error, RenderError::Conflict));
        let mut state = session.lock().expect("session");
        assert_eq!(state.current_ticket(id), Some(current));
        assert!(state.cached_view_snapshot(id).is_none());
    }
}
