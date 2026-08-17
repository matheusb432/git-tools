use std::sync::Arc;

use gtl_models::paths::ProjectName;
use gtl_wire::recipes::{Recipe, RecipeOp, RecipeTarget};

use super::{ViewerTabKind, recipe_label};
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
        label: computed_label(&recipe, &view),
        view: Arc::new(view),
    }
}

fn computed_label(recipe: &Recipe, view: &View) -> String {
    if let Some(name) = &recipe.name {
        return name.to_string();
    }

    let repo = &view.repo_name;
    match &recipe.op {
        RecipeOp::Diff { target } => computed_diff_label(repo, target, view),
        RecipeOp::MergeDiff { .. } => merge_label(repo, view),
    }
}

fn computed_diff_label(repo: &ProjectName, target: &RecipeTarget, view: &View) -> String {
    match target {
        RecipeTarget::Unpushed { .. } => {
            format!("{repo}: {}", recipe_label::commit_count(view.commits.len()))
        }
        RecipeTarget::Base { rev } => format!("{repo}: {rev}->working"),
        RecipeTarget::Range { range, .. } => format!("{repo}: {range}"),
        RecipeTarget::Merge { .. } => merge_label(repo, view),
        RecipeTarget::Last { count, .. } => format!(
            "{repo}: last {}",
            recipe_label::commit_count(count.get() as usize)
        ),
    }
}

fn merge_label(repo: &ProjectName, view: &View) -> String {
    format!("{repo}: merge {}->{}", view.branch, view.upstream)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use gtl_wire::recipes::{RecipeOp, RecipeTarget};

    use super::*;
    use crate::{
        utils::{
            diffs::commit,
            viewer::{empty_view, recipe},
        },
        viewer::complete_recipe_computation,
    };

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
                    target: RecipeTarget::Base {
                        rev: crate::utils::git_revision("v1"),
                    },
                },
                0,
                "project: v1->working",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Range {
                        range: crate::utils::git_range("v1..v2"),
                        pinned: None,
                    },
                },
                0,
                "project: v1..v2",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Merge {
                        base: crate::utils::git_revision("main"),
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
                    base: Some(crate::utils::git_revision("main")),
                    pinned: None,
                },
                0,
                "project: merge feature->main",
            ),
        ];

        for (op, commit_count, expected) in cases {
            let mut view = empty_view();
            view.commits = (0..commit_count).map(|_| commit("abc1234")).collect();
            let response = complete_recipe_computation::execute(CompleteRecipeComputation {
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
        named.name = Some(crate::utils::project_name("Release review"));

        let response = complete_recipe_computation::execute(CompleteRecipeComputation {
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
