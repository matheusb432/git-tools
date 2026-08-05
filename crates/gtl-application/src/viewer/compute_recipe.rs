use gtl_contracts::recipes::{self, Recipe, RecipeOp, RecipeTarget};

use crate::{
    diffs::{
        DiffTarget, PinnedRange, View,
        compute_diff::{self, ComputeDiff},
        compute_merge_diff::{self, ComputeMergeDiff},
    },
    ports::{GitClient, UserSettingsStore},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeRecipe {
    pub recipe: Recipe,
}

#[derive(Debug, Clone)]
pub struct ComputeRecipeOk {
    pub view: View,
}

#[derive(Debug, thiserror::Error)]
pub enum ComputeRecipeError {
    #[error(transparent)]
    Diff(#[from] compute_diff::ComputeDiffError),
    #[error(transparent)]
    MergeDiff(#[from] compute_merge_diff::ComputeMergeDiffError),
}

#[cqrsy::query]
pub fn execute(
    query: ComputeRecipe,
    user_settings: &impl UserSettingsStore,
    source: &impl GitClient,
) -> Result<ComputeRecipeOk, ComputeRecipeError> {
    let recipe = query.recipe;
    let cwd = recipe.cwd();
    let view = match recipe.op {
        RecipeOp::Diff { target } => {
            compute_diff::execute(
                ComputeDiff {
                    cwd,
                    target: diff_target(target),
                },
                user_settings,
                source,
            )?
            .view
        }
        RecipeOp::MergeDiff { base, pinned } => {
            compute_merge_diff::execute(
                ComputeMergeDiff {
                    cwd,
                    base,
                    pinned: pinned.map(application_pin),
                },
                user_settings,
                source,
            )?
            .view
        }
    };

    Ok(ComputeRecipeOk { view })
}

fn diff_target(target: RecipeTarget) -> DiffTarget {
    match target {
        RecipeTarget::Unpushed { pinned } => DiffTarget::Unpushed {
            pinned: pinned.map(application_pin),
        },
        RecipeTarget::Base { rev } => DiffTarget::Base(rev),
        RecipeTarget::Range { range, pinned } => DiffTarget::Range {
            range,
            pinned: pinned.map(application_pin),
        },
        RecipeTarget::Merge { base, pinned } => DiffTarget::Merge {
            base,
            pinned: pinned.map(application_pin),
        },
        RecipeTarget::Last { count, pinned } => DiffTarget::Last {
            count,
            pinned: pinned.map(application_pin),
        },
    }
}

fn application_pin(pin: recipes::PinnedRange) -> PinnedRange {
    PinnedRange {
        base: pin.base,
        head: pin.head,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::testing::{FakeGitClient, FixedUserSettingsStore, viewer::recipe};

    fn source() -> FakeGitClient {
        FakeGitClient {
            top_level: Some("/repos/project".into()),
            branch: "feature".into(),
            upstream: Some("main".into()),
            known_revs: vec![
                "v1".into(),
                "v2".into(),
                "release".into(),
                "HEAD~1".into(),
                "HEAD".into(),
            ],
            ..Default::default()
        }
    }

    fn pin() -> recipes::PinnedRange {
        recipes::PinnedRange {
            base: "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd".into(),
            head: "1111111111222222222233333333334444444444".into(),
        }
    }

    #[test]
    fn unpushed_diff_recipe_computes_a_view() {
        let source = source();

        let response = execute(
            ComputeRecipe {
                recipe: recipe(RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                }),
            },
            &FixedUserSettingsStore::default(),
            &source,
        )
        .expect("recipe computes");

        assert_eq!(response.view.repo_name, "project");
        assert_eq!(response.view.branch, "feature");
        assert_eq!(response.view.upstream, "main");
    }

    #[test]
    fn diff_recipe_targets_preserve_their_ranges() {
        let cases = [
            (RecipeTarget::Base { rev: "v1".into() }, "diff", "v1", "v1"),
            (
                RecipeTarget::Range {
                    range: "v1..v2".into(),
                    pinned: None,
                },
                "diff",
                "v1..v2",
                "v1..v2",
            ),
            (
                RecipeTarget::Merge {
                    base: "release".into(),
                    pinned: None,
                },
                "merge-diff",
                "release...HEAD",
                "release",
            ),
            (
                RecipeTarget::Last {
                    count: NonZeroU32::new(1).expect("non-zero"),
                    pinned: None,
                },
                "diff",
                "HEAD~1..HEAD",
                "HEAD~1..HEAD",
            ),
        ];
        let source = source();

        for (target, title, range, upstream) in cases {
            let response = execute(
                ComputeRecipe {
                    recipe: recipe(RecipeOp::Diff { target }),
                },
                &FixedUserSettingsStore::default(),
                &source,
            )
            .expect("recipe computes");

            assert_eq!(response.view.title, title);
            assert_eq!(response.view.cmd.range, range);
            assert_eq!(response.view.upstream, upstream);
        }
    }

    #[test]
    fn pinned_diff_recipe_maps_the_contract_pin() {
        let source = FakeGitClient {
            top_level: Some("/repos/project".into()),
            branch: "feature".into(),
            ..Default::default()
        };

        let response = execute(
            ComputeRecipe {
                recipe: recipe(RecipeOp::Diff {
                    target: RecipeTarget::Range {
                        range: "symbolic..range".into(),
                        pinned: Some(pin()),
                    },
                }),
            },
            &FixedUserSettingsStore::default(),
            &source,
        )
        .expect("pinned recipe computes");

        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
    }

    #[test]
    fn recipe_operations_dispatch_to_their_view_queries() {
        let source = FakeGitClient {
            top_level: Some("/repos/project".into()),
            branch: "feature".into(),
            ..Default::default()
        };
        let response = execute(
            ComputeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: Some("release".into()),
                    pinned: Some(pin()),
                }),
            },
            &FixedUserSettingsStore::default(),
            &source,
        )
        .expect("recipe computes");

        assert_eq!(response.view.title, "merge-diff");
    }

    #[test]
    fn operation_errors_identify_the_failed_recipe_kind() {
        let source = FakeGitClient {
            top_level: Some("/repos/project".into()),
            branch: "feature".into(),
            ..Default::default()
        };

        let diff = execute(
            ComputeRecipe {
                recipe: recipe(RecipeOp::Diff {
                    target: RecipeTarget::Base {
                        rev: "unknown".into(),
                    },
                }),
            },
            &FixedUserSettingsStore::default(),
            &source,
        )
        .expect_err("unknown diff base fails");
        let merge = execute(
            ComputeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: Some("unknown".into()),
                    pinned: None,
                }),
            },
            &FixedUserSettingsStore::default(),
            &source,
        )
        .expect_err("unknown merge base fails");
        assert!(matches!(diff, ComputeRecipeError::Diff(_)));
        assert!(matches!(merge, ComputeRecipeError::MergeDiff(_)));
    }
}
