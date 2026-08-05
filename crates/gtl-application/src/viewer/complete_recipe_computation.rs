use std::sync::Arc;

use gtl_contracts::recipes::Recipe;

use super::{ViewerTabKind, logic::recipe_label};
use crate::diffs::View;

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteRecipeComputation {
    pub recipe: Recipe,
    pub kind: ViewerTabKind,
    pub view: View,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompleteRecipeComputationOk {
    Publish { label: String, view: Arc<View> },
    Skipped { label: String },
}

#[cqrsy::command]
pub fn execute(command: CompleteRecipeComputation) -> CompleteRecipeComputationOk {
    let CompleteRecipeComputation { recipe, kind, view } = command;
    if kind == ViewerTabKind::Snapshot && !view.has_diff_content() {
        return CompleteRecipeComputationOk::Skipped {
            label: recipe_label::initial(&recipe),
        };
    }

    CompleteRecipeComputationOk::Publish {
        label: recipe_label::computed(&recipe, &view),
        view: Arc::new(view),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use gtl_contracts::recipes::{RecipeOp, RecipeTarget};
    use gtl_models::diffs::Commit;

    use super::*;
    use crate::testing::viewer::{empty_view, recipe};

    #[test]
    fn computed_labels_preserve_recipe_intent() {
        let cases = [
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                1,
                "project: 1 commit",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Base { rev: "v1".into() },
                },
                0,
                "project: v1->working",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Range {
                        range: "v1..v2".into(),
                        pinned: None,
                    },
                },
                0,
                "project: v1..v2",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Merge {
                        base: "main".into(),
                        pinned: None,
                    },
                },
                0,
                "project: merge feature->main",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Last {
                        count: NonZeroU32::new(1).expect("non-zero"),
                        pinned: None,
                    },
                },
                0,
                "project: last 1 commit",
            ),
            (
                RecipeOp::MergeDiff {
                    base: Some("main".into()),
                    pinned: None,
                },
                0,
                "project: merge feature->main",
            ),
        ];

        for (op, commit_count, expected) in cases {
            let mut view = empty_view();
            view.commits = (0..commit_count).map(|_| Commit::default()).collect();
            let response = execute(CompleteRecipeComputation {
                recipe: recipe(op),
                kind: ViewerTabKind::Live,
                view,
            });
            let CompleteRecipeComputationOk::Publish { label, .. } = response else {
                panic!("live recipe must publish");
            };

            assert_eq!(label, expected);
        }
    }

    #[test]
    fn explicit_name_overrides_the_computed_label() {
        let mut named = recipe(RecipeOp::MergeDiff {
            base: None,
            pinned: None,
        });
        named.name = Some("Release review".into());

        let response = execute(CompleteRecipeComputation {
            recipe: named,
            kind: ViewerTabKind::Live,
            view: empty_view(),
        });

        assert!(matches!(
            response,
        CompleteRecipeComputationOk::Publish { label, .. }
                if label == "Release review"
        ));
    }
}
