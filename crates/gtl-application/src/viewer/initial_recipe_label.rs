use super::recipe_label;
use crate::recipes::Recipe;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialRecipeLabel {
    pub recipe: Recipe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialRecipeLabelOk {
    pub label: String,
}

#[cqrsy::query]
pub fn execute(query: InitialRecipeLabel) -> InitialRecipeLabelOk {
    let InitialRecipeLabel { recipe } = query;
    InitialRecipeLabelOk {
        label: recipe_label::initial(&recipe),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{
        recipes::{RecipeOp, RecipeSource, RecipeTarget},
        utils::viewer::recipe,
        viewer::initial_recipe_label,
    };

    #[test]
    fn labels_preserve_recipe_intent() {
        let cases = [
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                "project: diff",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Base {
                        rev: crate::utils::git_revision("v1"),
                    },
                },
                "project: v1->working",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Range {
                        range: crate::utils::git_range("v1..v2"),
                        pinned: None,
                    },
                },
                "project: v1..v2",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Merge {
                        base: crate::utils::git_revision("release"),
                        pinned: None,
                    },
                },
                "project: merge ->release",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Last {
                        count: NonZeroU32::new(1).expect("non-zero"),
                        pinned: None,
                    },
                },
                "project: last 1 commit",
            ),
            (
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
                "project: merge ->main",
            ),
            (
                RecipeOp::MergeDiff {
                    base: Some(crate::utils::git_revision("release")),
                    pinned: None,
                },
                "project: merge ->release",
            ),
        ];

        for (op, expected) in cases {
            let response = initial_recipe_label::execute(InitialRecipeLabel { recipe: recipe(op) });

            assert_eq!(response.label, expected);
        }
    }

    #[test]
    fn explicit_name_overrides_the_recipe_label() {
        let mut named = recipe(RecipeOp::MergeDiff {
            base: None,
            pinned: None,
        });
        named.name = Some(crate::utils::project_name("Release review"));

        let response = initial_recipe_label::execute(InitialRecipeLabel { recipe: named });

        assert_eq!(response.label, "Release review");
    }

    #[test]
    fn root_repository_uses_its_full_path_as_the_label() {
        let mut root = recipe(RecipeOp::MergeDiff {
            base: None,
            pinned: None,
        });
        root.source = RecipeSource::LocalRepo(crate::utils::repository_root("/"));

        let response = initial_recipe_label::execute(InitialRecipeLabel { recipe: root });

        assert_eq!(response.label, "repo: merge ->main");
    }
}
