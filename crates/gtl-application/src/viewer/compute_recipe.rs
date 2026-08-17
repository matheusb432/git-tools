#[cfg(test)]
use gtl_wire::recipes;
use gtl_wire::recipes::{Recipe, RecipeOp, RecipeTarget};

use crate::{
    diffs::{
        DiffTarget, View,
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
                    repo_root: cwd,
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
                    repo_root: cwd,
                    base,
                    pinned,
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
        RecipeTarget::Unpushed { pinned } => DiffTarget::Unpushed { pinned },
        RecipeTarget::Base { rev } => DiffTarget::Base(rev),
        RecipeTarget::Range { range, pinned } => DiffTarget::Range { range, pinned },
        RecipeTarget::Merge { base, pinned } => DiffTarget::Merge { base, pinned },
        RecipeTarget::Last { count, pinned } => DiffTarget::Last { count, pinned },
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{
        utils::{FakeGitClient, FixedUserSettingsStore, viewer::recipe},
        viewer::compute_recipe,
    };

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
        crate::utils::pinned_range(
            "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd",
            "1111111111222222222233333333334444444444",
        )
    }

    #[test]
    fn unpushed_diff_recipe_computes_a_view() {
        let source = source();

        let response = compute_recipe::execute(
            ComputeRecipe {
                recipe: recipe(RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                }),
            },
            &FixedUserSettingsStore::default(),
            &source,
        )
        .expect("recipe computes");

        assert_eq!(response.view.repo_name.as_str(), "project");
        assert_eq!(response.view.branch.to_string(), "feature");
        assert_eq!(response.view.upstream.as_ref(), "main");
    }

    #[test]
    fn diff_recipe_targets_preserve_their_ranges() {
        let cases = [
            (
                RecipeTarget::Base {
                    rev: crate::utils::git_revision("v1"),
                },
                "diff",
                "7631763176",
                "7631763176",
            ),
            (
                RecipeTarget::Range {
                    range: crate::utils::git_range("v1..v2"),
                    pinned: None,
                },
                "diff",
                "v1..v2",
                "v1..v2",
            ),
            (
                RecipeTarget::Merge {
                    base: crate::utils::git_revision("release"),
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
            let response = compute_recipe::execute(
                ComputeRecipe {
                    recipe: recipe(RecipeOp::Diff { target }),
                },
                &FixedUserSettingsStore::default(),
                &source,
            )
            .expect("recipe computes");

            assert_eq!(response.view.title, title);
            assert_eq!(response.view.cmd.range, range);
            assert_eq!(response.view.upstream.as_ref(), upstream);
        }
    }

    #[test]
    fn pinned_diff_recipe_maps_the_contract_pin() {
        let source = FakeGitClient {
            top_level: Some("/repos/project".into()),
            branch: "feature".into(),
            ..Default::default()
        };

        let response = compute_recipe::execute(
            ComputeRecipe {
                recipe: recipe(RecipeOp::Diff {
                    target: RecipeTarget::Range {
                        range: crate::utils::git_range("symbolic..range"),
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
        let response = compute_recipe::execute(
            ComputeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: Some(crate::utils::git_revision("release")),
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

        let diff = compute_recipe::execute(
            ComputeRecipe {
                recipe: recipe(RecipeOp::Diff {
                    target: RecipeTarget::Base {
                        rev: crate::utils::git_revision("unknown"),
                    },
                }),
            },
            &FixedUserSettingsStore::default(),
            &source,
        )
        .expect_err("unknown diff base fails");
        let merge = compute_recipe::execute(
            ComputeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: Some(crate::utils::git_revision("unknown")),
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
