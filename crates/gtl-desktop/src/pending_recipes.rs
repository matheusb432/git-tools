use std::collections::VecDeque;

use gtl_application::viewer::ViewerTabKind;
use gtl_contracts::recipes::{OpenRecipes, Recipe, RecipeBatchKind};

use crate::{presentation::ViewerApp, recipes::RecipeError, session::PendingRecipesError};

#[derive(Debug)]
pub(crate) enum ProcessPendingRecipesError {
    Queue(PendingRecipesError),
    Open(RecipeError),
    Restore {
        open: RecipeError,
        queue: PendingRecipesError,
    },
}

impl std::fmt::Display for ProcessPendingRecipesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queue(error) => error.fmt(formatter),
            Self::Open(error) => error.fmt(formatter),
            Self::Restore { open, queue } => {
                write!(
                    formatter,
                    "{open}; failed to restore pending recipes: {queue}"
                )
            }
        }
    }
}

impl std::error::Error for ProcessPendingRecipesError {}

impl ViewerApp {
    pub(crate) fn enqueue_and_process_recipes(
        &self,
        batches: Vec<OpenRecipes>,
    ) -> Result<(), ProcessPendingRecipesError> {
        if batches.is_empty() {
            return Ok(());
        }

        self.pending()
            .with_consumer(|| {
                for batch in batches {
                    self.pending()
                        .push(batch)
                        .map_err(ProcessPendingRecipesError::Queue)?;
                }
                process_transaction(self)
            })
            .map_err(ProcessPendingRecipesError::Queue)??;
        Ok(())
    }

    pub(crate) fn process_pending_recipes(&self) -> Result<(), ProcessPendingRecipesError> {
        self.pending()
            .with_consumer(|| process_transaction(self))
            .map_err(ProcessPendingRecipesError::Queue)?
    }
}

fn process_transaction(app: &ViewerApp) -> Result<(), ProcessPendingRecipesError> {
    let batches = app
        .pending()
        .try_drain()
        .map_err(ProcessPendingRecipesError::Queue)?;
    match process(batches, |recipe, batch_id, kind| {
        app.open_recipe(recipe, batch_id.into(), viewer_tab_kind(kind))
    }) {
        Ok(()) => Ok(()),
        Err(failure) => match app.pending().prepend(failure.remainder) {
            Ok(()) => Err(ProcessPendingRecipesError::Open(failure.reason)),
            Err(queue) => Err(ProcessPendingRecipesError::Restore {
                open: failure.reason,
                queue,
            }),
        },
    }
}

#[derive(Debug)]
struct PendingFailure<E> {
    reason: E,
    remainder: Vec<OpenRecipes>,
}

fn process<T, E>(
    batches: Vec<OpenRecipes>,
    mut open: impl FnMut(&Recipe, &str, RecipeBatchKind) -> Result<T, E>,
) -> Result<(), PendingFailure<E>> {
    let mut batches = VecDeque::from(batches);
    while let Some(mut batch) = batches.pop_front() {
        let mut recipes = VecDeque::from(std::mem::take(&mut batch.recipes));
        while let Some(recipe) = recipes.pop_front() {
            if let Err(reason) = open(&recipe, &batch.batch_id, batch.kind) {
                recipes.push_front(recipe);
                batch.recipes = recipes.into_iter().collect();
                let mut remainder = vec![batch];
                remainder.extend(batches);
                return Err(PendingFailure { reason, remainder });
            }
        }
    }
    Ok(())
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
                Ok(path)
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
    fn processing_visits_batches_and_recipes_in_fifo_order() {
        let batches = vec![
            OpenRecipes {
                batch_id: "first".into(),
                kind: RecipeBatchKind::Snapshot,
                recipes: vec![named("/one"), named("/two")],
            },
            OpenRecipes {
                batch_id: "second".into(),
                kind: RecipeBatchKind::Live,
                recipes: vec![named("/three")],
            },
        ];
        let mut opened = Vec::new();

        process(batches, |recipe, batch, kind| {
            opened.push((batch.to_owned(), recipe.cwd().display().to_string(), kind));
            Ok::<_, String>(())
        })
        .expect("all recipes open");

        assert_eq!(
            opened,
            [
                ("first".into(), "/one".into(), RecipeBatchKind::Snapshot),
                ("first".into(), "/two".into(), RecipeBatchKind::Snapshot),
                ("second".into(), "/three".into(), RecipeBatchKind::Live),
            ]
        );
    }

    #[test]
    fn batch_kind_maps_exhaustively_to_tab_kind() {
        assert_eq!(
            viewer_tab_kind(RecipeBatchKind::Snapshot),
            ViewerTabKind::Snapshot
        );
        assert_eq!(viewer_tab_kind(RecipeBatchKind::Live), ViewerTabKind::Live);
    }
}
