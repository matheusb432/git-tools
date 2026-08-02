use gtl_application::{
    history::{RecentRenderRecord, get_recent_render, list_recent_renders},
    viewer::{RenderHistoryId, ViewerHistoryEntry, ViewerTabKind},
};
use tauri::http::StatusCode;

use super::{
    response::{RouteOutput, RouteResult},
    settings, tabs,
};
use crate::{presentation::ViewerApp, render::SwapFeedback};

pub(super) fn list(app: &ViewerApp) -> RouteResult {
    let entries = load(app)?;
    Ok(RouteOutput::Html(app.renderer.build_history(&entries)))
}

pub(super) fn open(app: &ViewerApp, id: RenderHistoryId) -> RouteResult {
    let Some(entry) =
        get_recent_render::execute(get_recent_render::GetRecentRender { id }, &app.app_state)
            .map_err(|error| format!("{error:#}"))?
            .entry
    else {
        return Ok(RouteOutput::Empty(StatusCode::NOT_FOUND));
    };
    app.open_recipe(
        &entry.recipe,
        format!("history-{id}"),
        ViewerTabKind::Snapshot,
    )?;
    let settings = settings::load(app);
    tabs::render_tabs_only(app.renderer, &app.session, settings, SwapFeedback::None)
        .map(RouteOutput::Html)
        .map_err(Into::into)
}

pub(super) fn load(app: &ViewerApp) -> Result<Vec<ViewerHistoryEntry>, String> {
    list_recent_renders::execute(list_recent_renders::ListRecentRenders, &app.app_state)
        .map(|response| response.entries.into_iter().map(to_viewer_entry).collect())
        .map_err(|error| format!("{error:#}"))
}

fn to_viewer_entry(record: RecentRenderRecord) -> ViewerHistoryEntry {
    ViewerHistoryEntry::new(
        record.id,
        record.title,
        record.repo_name,
        record.range_label,
        record.rendered_at,
        record.recipe,
    )
}

#[cfg(test)]
mod tests {
    use gtl_application::{history::RecentRenderRecord, viewer::RenderHistoryId};

    use super::*;

    #[test]
    fn recent_render_mapping_preserves_viewer_fields() {
        let id = RenderHistoryId::try_new(7).expect("positive id");
        let entry = to_viewer_entry(RecentRenderRecord {
            id,
            title: "Named diff".into(),
            repo_name: "git-tools".into(),
            range_label: "main...feature".into(),
            rendered_at: "2026-07-11T10:00:00Z".into(),
            recipe: gtl_contracts::recipes::Recipe {
                source: gtl_contracts::recipes::RecipeSource::LocalRepo("/repos/gt".into()),
                op: gtl_contracts::recipes::RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
                name: None,
            },
        });

        assert_eq!(entry.id(), id);
        assert_eq!(entry.title(), "Named diff");
        assert_eq!(entry.repo_name(), "git-tools");
        assert_eq!(entry.kind(), "merge-diff");
        assert_eq!(entry.range_label(), "main...feature");
        assert_eq!(entry.rendered_at(), "2026-07-11T10:00:00Z");
    }
}
