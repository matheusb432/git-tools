use std::path::PathBuf;

use application::{history::RecentRenderRecord, viewer::RenderHistoryId};
use contracts::recipes::{OpenRecipes, Recipe, RecipeBatchKind, RecipeOp, RecipeSource};
use tauri::http::StatusCode;

use super::{
    ErrorTarget, PendingRecipeOutcome, RouteError, error_response, history::to_viewer_entry,
    process_pending,
};
use crate::render::VIEW_STATE_ERROR;

fn named(path: &str) -> Recipe {
    Recipe {
        source: RecipeSource::LocalRepo(path.into()),
        op: RecipeOp::SquashPreview { pinned: None },
        name: None,
    }
}

#[test]
fn recent_render_mapping_preserves_viewer_fields() {
    let id = RenderHistoryId::try_new(7).expect("positive id");
    let entry = to_viewer_entry(RecentRenderRecord {
        id,
        title: "Named diff".into(),
        repo_name: "git-tools".into(),
        range_label: "main...feature".into(),
        rendered_at: "2026-07-11T10:00:00Z".into(),
        recipe: contracts::recipes::Recipe {
            source: contracts::recipes::RecipeSource::LocalRepo("/repos/gt".into()),
            op: contracts::recipes::RecipeOp::MergeDiff {
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

#[test]
fn pending_processing_preserves_fifo_failure_remainder_for_retry() {
    let batches = vec![
        OpenRecipes {
            batch_id: "first".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![named("/one"), named("/fail"), named("/three")],
        },
        OpenRecipes {
            batch_id: "second".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![named("/four")],
        },
    ];
    let mut opened = Vec::new();
    let failure = process_pending(batches, |recipe, batch, _kind| {
        let path = recipe.cwd().display().to_string();
        opened.push((batch.to_string(), path.clone()));
        if path == "/fail" {
            Err(String::from("boom"))
        } else {
            Ok(PendingRecipeOutcome::Opened(path))
        }
    })
    .expect_err("middle recipe fails");

    assert_eq!(
        opened,
        vec![
            ("first".into(), "/one".into()),
            ("first".into(), "/fail".into())
        ]
    );
    assert_eq!(failure.remainder[0].recipes.len(), 2);
    assert_eq!(
        failure.remainder[0].recipes[0].cwd(),
        PathBuf::from("/fail")
    );
    assert_eq!(failure.remainder[1].batch_id, "second");
}

#[test]
fn pending_processing_preserves_opened_and_skipped_results_independently() {
    let batches = vec![OpenRecipes {
        batch_id: "batch".into(),
        kind: RecipeBatchKind::Snapshot,
        recipes: vec![named("/one"), named("/skip"), named("/three")],
    }];

    let processed = process_pending(batches, |recipe, _batch, _kind| {
        let path = recipe.cwd().display().to_string();
        Ok::<_, String>(if path == "/skip" {
            PendingRecipeOutcome::Skipped("skip label".into())
        } else {
            PendingRecipeOutcome::Opened(path)
        })
    })
    .expect("batch succeeds");

    assert_eq!(processed.latest_opened.as_deref(), Some("/three"));
    assert_eq!(processed.skipped_labels, ["skip label"]);
}

#[test]
fn conflict_and_internal_errors_keep_target_roots_and_hide_details() {
    for (target, error, status, root) in [
        (
            ErrorTarget::View,
            RouteError::Conflict,
            StatusCode::CONFLICT,
            "<section id=\"viewer-view\"",
        ),
        (
            ErrorTarget::Tabs,
            RouteError::Internal("sqlite /secret/path".into()),
            StatusCode::INTERNAL_SERVER_ERROR,
            "<nav id=\"viewer-tabs\"",
        ),
    ] {
        let response = error_response(target, &error);
        let html = String::from_utf8_lossy(response.body());
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["X-GTL-Recovery"], "true");
        assert_eq!(response.headers()["HX-Reswap"], "outerHTML");
        assert!(html.starts_with(root));
        assert!(!html.contains("sqlite"));
        assert!(!html.contains("/secret/path"));
    }
}

#[test]
fn action_errors_return_empty_non_swappable_responses() {
    for (error, expected_status) in [
        (RouteError::NotFound, StatusCode::NOT_FOUND),
        (
            RouteError::Internal("editor /secret/path".into()),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ] {
        let response = error_response(ErrorTarget::Action, &error);
        assert_eq!(response.status(), expected_status);
        assert!(response.body().is_empty());
        assert!(!response.headers().contains_key("X-GTL-Recovery"));
        assert!(!response.headers().contains_key("HX-Reswap"));
    }
}

#[test]
fn recovery_fragments_own_their_error_presentation() {
    let view = super::render::error_view();
    let tabs = super::render::error_tabs();
    let history = super::render::error_history();

    assert!(view.contains(&format!("data-viewer-state=\"{VIEW_STATE_ERROR}\"")));
    assert!(view.contains("class=\"viewer-status "));
    assert!(tabs.contains("class=\"viewer-tabs "));
    assert!(tabs.contains("border-del-line"));
    assert!(history.contains("class=\"viewer-history "));
    assert!(history.contains("border-del-line"));
}
