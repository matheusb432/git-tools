use std::collections::VecDeque;

use gtl_application::viewer::ViewerTabKind;
use gtl_contracts::recipes::{OpenRecipes, Recipe, RecipeBatchKind};

use super::{
    response::{RouteError, RouteOutput, RouteResult},
    settings, tabs,
};
use crate::{presentation::ViewerApp, render::SwapFeedback};

pub(super) fn serve(app: &ViewerApp) -> RouteResult {
    app.pending().with_consumer(|| process_transaction(app))?
}

fn process_transaction(app: &ViewerApp) -> RouteResult {
    let batches = app.pending().try_drain()?;
    match process(batches, |recipe, batch_id, kind| {
        app.open_recipe(recipe, batch_id.into(), viewer_tab_kind(kind))
            .map(PendingRecipeOutcome::Opened)
    }) {
        Ok(processed) => {
            let PendingProcess {
                latest_opened: _,
                skipped_labels,
            } = processed;
            let settings = settings::load(app)?;
            let feedback = if skipped_labels.is_empty() {
                SwapFeedback::None
            } else {
                SwapFeedback::SnapshotRecipesSkipped(&skipped_labels)
            };
            let html = tabs::render_tabs_only(app.renderer, &app.session, settings, feedback)?;
            Ok(RouteOutput::Html(html))
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
    #[cfg(test)]
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
    remainder: Vec<OpenRecipes>,
}

fn process<T, E>(
    batches: Vec<OpenRecipes>,
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
                #[cfg(test)]
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_contracts::recipes::{OpenRecipes, Recipe, RecipeBatchKind, RecipeOp, RecipeSource};

    use super::*;

    fn named(path: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(path.into()),
            op: RecipeOp::MergeDiff {
                base: None,
                pinned: None,
            },
            name: None,
        }
    }

    #[test]
    fn processing_preserves_fifo_failure_remainder_for_retry() {
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
        let failure = process(batches, |recipe, batch, _kind| {
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
    fn processing_preserves_opened_and_skipped_results_independently() {
        let batches = vec![OpenRecipes {
            batch_id: "batch".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![named("/one"), named("/skip"), named("/three")],
        }];

        let processed = process(batches, |recipe, _batch, _kind| {
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
}
