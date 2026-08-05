use gtl_contracts::recipes::Recipe;

use super::logic::recipe_label;

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

    use gtl_contracts::recipes::{RecipeOp, RecipeSource, RecipeTarget};

    use super::*;
    use crate::testing::viewer::recipe;

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
                    target: RecipeTarget::Base { rev: "v1".into() },
                },
                "project: v1->working",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Range {
                        range: "v1..v2".into(),
                        pinned: None,
                    },
                },
                "project: v1..v2",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Merge {
                        base: String::new(),
                        pinned: None,
                    },
                },
                "project: merge",
            ),
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Merge {
                        base: "release".into(),
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
                    base: Some("  ".into()),
                    pinned: None,
                },
                "project: merge ->main",
            ),
            (
                RecipeOp::MergeDiff {
                    base: Some("release".into()),
                    pinned: None,
                },
                "project: merge ->release",
            ),
        ];

        for (op, expected) in cases {
            let response = execute(InitialRecipeLabel { recipe: recipe(op) });

            assert_eq!(response.label, expected);
        }
    }

    #[test]
    fn explicit_name_overrides_the_recipe_label() {
        let mut named = recipe(RecipeOp::MergeDiff {
            base: None,
            pinned: None,
        });
        named.name = Some("Release review".into());

        let response = execute(InitialRecipeLabel { recipe: named });

        assert_eq!(response.label, "Release review");
    }

    #[test]
    fn root_repository_uses_its_full_path_as_the_label() {
        let mut root = recipe(RecipeOp::MergeDiff {
            base: None,
            pinned: None,
        });
        root.source = RecipeSource::LocalRepo("/".into());

        let response = execute(InitialRecipeLabel { recipe: root });

        assert_eq!(response.label, "/: merge ->main");
    }
}
