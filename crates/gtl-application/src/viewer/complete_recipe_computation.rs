use std::sync::Arc;

use gtl_models::{git::GitRevision, recipes::RecipeLabel};

use crate::{
    diffs::View,
    recipes::{Recipe, recipe_label},
};

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteRecipeComputation {
    pub recipe: Recipe,
    pub view: View,
    /// Names the upstream or comparison branch an unpushed recipe compared against.
    pub comparison_name: Option<GitRevision>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteRecipeComputationOk {
    pub label: RecipeLabel,
    pub view: Arc<View>,
}

#[cqrsy::command]
pub fn execute(command: CompleteRecipeComputation) -> CompleteRecipeComputationOk {
    let CompleteRecipeComputation {
        recipe,
        view,
        comparison_name,
    } = command;
    CompleteRecipeComputationOk {
        label: recipe_label::compared(&recipe, &view, comparison_name.as_ref()),
        view: Arc::new(view),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use gtl_models::{git::GitHead, recipes::RecipeLabelHead};

    use super::*;
    use crate::{
        recipes::{RecipeOp, RecipeTarget},
        utils::{
            git_range, git_revision, pinned_range, project_name,
            viewer::{empty_view, recipe},
        },
        viewer::complete_recipe_computation,
    };

    fn label(op: RecipeOp, comparison_name: Option<&str>) -> RecipeLabel {
        let mut view = empty_view();
        view.branch = GitHead::try_from("feature".to_owned()).unwrap();
        complete_recipe_computation::execute(CompleteRecipeComputation {
            recipe: recipe(op),
            view,
            comparison_name: comparison_name.map(git_revision),
        })
        .label
    }

    fn compared(base: &str, head: RecipeLabelHead) -> RecipeLabel {
        RecipeLabel::Compared {
            repository: project_name("project"),
            base: git_revision(base),
            head,
        }
    }

    fn revision(head: &str) -> RecipeLabelHead {
        RecipeLabelHead::Revision {
            revision: git_revision(head),
        }
    }

    #[test]
    fn tabs_name_the_compared_branches_when_they_are_known() {
        let pin = pinned_range(&"a".repeat(40), &"b".repeat(40));
        let unpushed = |pinned| RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned },
        };

        assert_eq!(
            label(unpushed(Some(pin.clone())), Some("origin/feature")),
            compared("origin/feature", revision("feature"))
        );
        assert_eq!(
            label(unpushed(Some(pin)), None),
            compared("aaaaaa", revision("feature"))
        );
        assert_eq!(
            label(
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
                None,
            ),
            compared("main", revision("feature"))
        );
    }

    #[test]
    fn tabs_shorten_commit_ids_and_keep_written_revisions() {
        assert_eq!(
            label(
                RecipeOp::Diff {
                    target: RecipeTarget::Base {
                        rev: git_revision("0123456789abcdef"),
                    },
                },
                None,
            ),
            compared("012345", RecipeLabelHead::WorkingTree)
        );
        assert_eq!(
            label(
                RecipeOp::Diff {
                    target: RecipeTarget::Range {
                        range: git_range("v1..v2"),
                        pinned: None,
                    },
                },
                None,
            ),
            compared("v1", revision("v2"))
        );
        assert_eq!(
            label(
                RecipeOp::Diff {
                    target: RecipeTarget::Last {
                        count: NonZeroU32::new(3).unwrap(),
                        pinned: None,
                    },
                },
                None,
            ),
            compared("HEAD~3", revision("feature"))
        );
    }

    #[test]
    fn explicit_name_overrides_the_computed_label() {
        let mut named = recipe(RecipeOp::MergeDiff {
            base: None,
            pinned: None,
        });
        named.name = Some(project_name("Release review"));

        let label = complete_recipe_computation::execute(CompleteRecipeComputation {
            recipe: named,
            view: empty_view(),
            comparison_name: None,
        })
        .label;

        assert_eq!(
            label,
            RecipeLabel::Named {
                name: project_name("Release review"),
            }
        );
    }
}
