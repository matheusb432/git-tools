use gtl_application::history::{
    RecentRenderRecord, get_recent_render,
    list_recent_render_page::{
        self, ListRecentRenderPage, ListRecentRenderPageOk, RecentRenderPageCursor,
    },
};
use gtl_models::{
    recipes::RecipeBatchId,
    viewer::{RenderHistoryId, ViewerTabKind},
};
use gtl_wire::{
    recipes::RecipeOp,
    viewer::{
        GetViewerHistoryCopy, ListViewerHistory, OpenViewerHistory, ViewerApiError,
        ViewerHistoryCopyKind, ViewerHistoryCopyPayload, ViewerHistoryCursor, ViewerHistoryEntry,
        ViewerHistoryPage, ViewerRecipeKind, ViewerResource, ViewerShell,
    },
};

use super::{internal, shell};
use crate::{presentation::ViewerApp, recipes::RecipeError};

pub(super) fn list(
    app: &ViewerApp,
    request: ListViewerHistory,
) -> Result<ViewerHistoryPage, ViewerApiError> {
    let cursor = to_cursor(request.cursor);
    let connection = app
        .app_state
        .connection_lock()
        .map_err(|error| internal("failed to lock render history", format_args!("{error:#}")))?;
    let page = list_recent_render_page::execute(ListRecentRenderPage { cursor }, &connection)
        .map_err(|error| internal("failed to list render history", error))?;
    Ok(to_history_page(page))
}

pub(super) fn open(
    app: &ViewerApp,
    request: OpenViewerHistory,
) -> Result<ViewerShell, ViewerApiError> {
    let entry = get_history_record(app, request.render_id)?;
    app.open_recipe(
        &entry.recipe,
        RecipeBatchId::generate(),
        ViewerTabKind::Snapshot,
    )
    .map_err(map_recipe_error)?;
    shell::load(app, None)
}

pub(super) fn copy(
    app: &ViewerApp,
    request: GetViewerHistoryCopy,
) -> Result<ViewerHistoryCopyPayload, ViewerApiError> {
    get_history_record(app, request.render_id).map(to_history_copy_payload)
}

fn get_history_record(
    app: &ViewerApp,
    id: RenderHistoryId,
) -> Result<RecentRenderRecord, ViewerApiError> {
    let connection = app
        .app_state
        .connection_lock()
        .map_err(|error| internal("failed to lock render history", format_args!("{error:#}")))?;
    get_recent_render::execute(&get_recent_render::GetRecentRender { id }, &connection)
        .map_err(|error| internal("failed to load render history entry", error))?
        .entry
        .ok_or(ViewerApiError::NotFound {
            resource: ViewerResource::HistoryEntry,
        })
}

fn to_cursor(cursor: ViewerHistoryCursor) -> RecentRenderPageCursor {
    match cursor {
        ViewerHistoryCursor::Newest => RecentRenderPageCursor::Newest,
        ViewerHistoryCursor::Oldest => RecentRenderPageCursor::Oldest,
        ViewerHistoryCursor::OlderThan { render_id, page } => RecentRenderPageCursor::OlderThan {
            render: render_id,
            page,
        },
        ViewerHistoryCursor::NewerThan { render_id, page } => RecentRenderPageCursor::NewerThan {
            render: render_id,
            page,
        },
    }
}

fn to_history_page(page: ListRecentRenderPageOk) -> ViewerHistoryPage {
    ViewerHistoryPage {
        entries: page.entries.into_iter().map(to_history_entry).collect(),
        total_count: page.total_count,
        position: page.position,
        has_newer: page.has_newer,
        has_older: page.has_older,
    }
}

fn to_history_entry(record: RecentRenderRecord) -> ViewerHistoryEntry {
    let kind = match &record.recipe.op {
        RecipeOp::Diff { .. } => ViewerRecipeKind::Diff,
        RecipeOp::MergeDiff { .. } => ViewerRecipeKind::MergeDiff,
    };
    ViewerHistoryEntry {
        id: record.id,
        title: record.title,
        repository_name: record.repo_name,
        kind,
        range_label: record.range_label,
        rendered_at: record.rendered_at,
    }
}

fn to_history_copy_payload(record: RecentRenderRecord) -> ViewerHistoryCopyPayload {
    let kind = match &record.recipe.op {
        RecipeOp::Diff { .. } => ViewerHistoryCopyKind::Diff,
        RecipeOp::MergeDiff { .. } => ViewerHistoryCopyKind::MergeDiff,
    };
    ViewerHistoryCopyPayload {
        id: record.id,
        title: record.title,
        repo_name: record.repo_name,
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
    use gtl_models::viewer::HistoryPageNumber;
    use gtl_wire::recipes::{Recipe, RecipeOp, RecipeSource};

    use super::*;
    use crate::testing::{project_name, repository_root};

    #[test]
    fn cursor_mapping_preserves_the_validated_page_role() {
        let page = HistoryPageNumber::try_new(2).expect("positive page number");

        assert_eq!(
            to_cursor(ViewerHistoryCursor::NewerThan {
                render_id: RenderHistoryId::try_new(1).expect("positive render ID"),
                page,
            }),
            RecentRenderPageCursor::NewerThan {
                render: RenderHistoryId::try_new(1).expect("positive render ID"),
                page,
            }
        );
    }

    fn record() -> RecentRenderRecord {
        RecentRenderRecord {
            id: RenderHistoryId::try_new(7).expect("positive render id"),
            title: "Saved diff".into(),
            repo_name: project_name("git-tools"),
            range_label: "main...feature".into(),
            rendered_at: gtl_models::timestamps::MachineTimestamp::try_from("2026-08-09T12:00:00Z")
                .expect("fixture render timestamp is valid"),
            recipe: Recipe {
                source: RecipeSource::LocalRepo(repository_root("/workspace/repository")),
                op: RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
                name: Some(project_name("saved")),
            },
        }
    }

    #[test]
    fn history_list_mapping_omits_the_reopen_recipe_and_source_path() {
        let entry = to_history_entry(record());
        let payload = serde_json::to_string(&entry);
        assert!(payload.is_ok());
        let Ok(payload) = payload else {
            return;
        };

        assert_eq!(i64::from(entry.id), 7);
        assert_eq!(entry.kind, ViewerRecipeKind::MergeDiff);
        assert!(!payload.contains("recipe"));
        assert!(!payload.contains("/workspace/repository"));
    }

    #[test]
    fn history_copy_mapping_preserves_the_established_explicit_payload() {
        let payload = to_history_copy_payload(record());
        let json = serde_json::to_value(&payload);
        assert!(json.is_ok());
        let Ok(json) = json else {
            return;
        };

        assert_eq!(json["repo_name"], "git-tools");
        assert!(json.get("repository_name").is_none());
        assert_eq!(json["kind"], "merge-diff");
        assert_eq!(json["recipe"]["source"]["value"], "/workspace/repository");
    }

    #[test]
    fn history_copy_kind_is_derived_from_the_recipe_operation() {
        let recipe = Recipe {
            source: RecipeSource::LocalRepo(repository_root("/repo")),
            op: RecipeOp::Diff {
                target: gtl_wire::recipes::RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        };
        let mut record = record();
        record.recipe = recipe;

        assert_eq!(
            to_history_copy_payload(record).kind,
            gtl_wire::viewer::ViewerHistoryCopyKind::Diff
        );
    }
}
