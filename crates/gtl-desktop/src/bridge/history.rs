use std::num::NonZeroUsize;

use gtl_application::history::{
    RecentRenderRecord, get_recent_render,
    list_recent_render_page::{
        self, ListRecentRenderPage, ListRecentRenderPageOk, RecentRenderPageCursor,
    },
};
use gtl_contracts::{
    recipes::RecipeOp,
    viewer::{
        ListViewerHistory, OpenViewerHistory, ViewerApiError, ViewerHistoryCursor,
        ViewerHistoryEntry, ViewerHistoryPage, ViewerRecipeKind, ViewerResource, ViewerShell,
    },
};
use gtl_models::viewer::{RenderHistoryId, ViewerTabKind};

use super::{internal, shell};
use crate::{presentation::ViewerApp, recipes::RecipeError};

pub(super) fn list(
    app: &ViewerApp,
    request: ListViewerHistory,
) -> Result<ViewerHistoryPage, ViewerApiError> {
    let cursor = to_cursor(request.cursor)?;
    let connection = app
        .app_state
        .connection_lock()
        .map_err(|error| internal("failed to lock render history", format_args!("{error:#}")))?;
    let page = list_recent_render_page::execute(ListRecentRenderPage { cursor }, &connection)
        .map_err(|error| internal("failed to list render history", error))?;
    to_history_page(page)
}

pub(super) fn open(
    app: &ViewerApp,
    request: OpenViewerHistory,
) -> Result<ViewerShell, ViewerApiError> {
    let id =
        RenderHistoryId::try_new(request.render_id).map_err(|_| ViewerApiError::InvalidRequest)?;
    let entry = {
        let connection = app.app_state.connection_lock().map_err(|error| {
            internal("failed to lock render history", format_args!("{error:#}"))
        })?;
        get_recent_render::execute(get_recent_render::GetRecentRender { id }, &connection)
            .map_err(|error| internal("failed to load render history entry", error))?
            .entry
            .ok_or(ViewerApiError::NotFound {
                resource: ViewerResource::HistoryEntry,
            })?
    };
    app.open_recipe(
        &entry.recipe,
        format!("history-{id}"),
        ViewerTabKind::Snapshot,
    )
    .map_err(map_recipe_error)?;
    shell::load(app, None)
}

fn to_cursor(cursor: ViewerHistoryCursor) -> Result<RecentRenderPageCursor, ViewerApiError> {
    match cursor {
        ViewerHistoryCursor::Newest => Ok(RecentRenderPageCursor::Newest),
        ViewerHistoryCursor::Oldest => Ok(RecentRenderPageCursor::Oldest),
        ViewerHistoryCursor::OlderThan { render_id, page } => {
            Ok(RecentRenderPageCursor::OlderThan {
                render: RenderHistoryId::try_new(render_id)
                    .map_err(|_| ViewerApiError::InvalidRequest)?,
                page: page_number(page)?,
            })
        }
        ViewerHistoryCursor::NewerThan { render_id, page } => {
            Ok(RecentRenderPageCursor::NewerThan {
                render: RenderHistoryId::try_new(render_id)
                    .map_err(|_| ViewerApiError::InvalidRequest)?,
                page: page_number(page)?,
            })
        }
    }
}

fn page_number(page: u32) -> Result<NonZeroUsize, ViewerApiError> {
    usize::try_from(page)
        .ok()
        .and_then(NonZeroUsize::new)
        .ok_or(ViewerApiError::InvalidRequest)
}

fn to_history_page(page: ListRecentRenderPageOk) -> Result<ViewerHistoryPage, ViewerApiError> {
    Ok(ViewerHistoryPage {
        entries: page.entries.into_iter().map(to_history_entry).collect(),
        total_count: u64::try_from(page.total_count)
            .map_err(|error| internal("history count exceeds the wire range", error))?,
        page_number: u32::try_from(page.page_number)
            .map_err(|error| internal("history page number exceeds the wire range", error))?,
        page_count: u32::try_from(page.page_count)
            .map_err(|error| internal("history page count exceeds the wire range", error))?,
        has_newer: page.has_newer,
        has_older: page.has_older,
    })
}

fn to_history_entry(record: RecentRenderRecord) -> ViewerHistoryEntry {
    let kind = match &record.recipe.op {
        RecipeOp::Diff { .. } => ViewerRecipeKind::Diff,
        RecipeOp::MergeDiff { .. } => ViewerRecipeKind::MergeDiff,
    };
    ViewerHistoryEntry {
        id: record.id.into(),
        title: record.title,
        repository_name: record.repo_name,
        kind,
        range_label: record.range_label,
        rendered_at: record.rendered_at,
        recipe: record.recipe,
    }
}

fn map_recipe_error(error: RecipeError) -> ViewerApiError {
    match error {
        RecipeError::Stale => ViewerApiError::Conflict,
        RecipeError::Failed(reason) => internal("failed to open render history entry", reason),
    }
}

#[cfg(test)]
mod tests {
    use gtl_contracts::recipes::{Recipe, RecipeOp, RecipeSource};

    use super::*;

    #[test]
    fn cursor_rejects_nonpositive_ids_and_zero_pages() {
        assert_eq!(
            to_cursor(ViewerHistoryCursor::OlderThan {
                render_id: 0,
                page: 1,
            }),
            Err(ViewerApiError::InvalidRequest)
        );
        assert_eq!(
            to_cursor(ViewerHistoryCursor::NewerThan {
                render_id: 1,
                page: 0,
            }),
            Err(ViewerApiError::InvalidRequest)
        );
    }

    #[test]
    fn history_entry_mapping_preserves_the_reopen_recipe() {
        let recipe = Recipe {
            source: RecipeSource::LocalRepo("/repo".into()),
            op: RecipeOp::MergeDiff {
                base: None,
                pinned: None,
            },
            name: Some("saved".into()),
        };
        let entry = to_history_entry(RecentRenderRecord {
            id: RenderHistoryId::try_new(7).expect("positive render id"),
            title: "Saved diff".into(),
            repo_name: "git-tools".into(),
            range_label: "main...feature".into(),
            rendered_at: "2026-08-09T12:00:00Z".into(),
            recipe: recipe.clone(),
        });

        assert_eq!(entry.id, 7);
        assert_eq!(entry.kind, ViewerRecipeKind::MergeDiff);
        assert_eq!(entry.recipe, recipe);
    }
}
