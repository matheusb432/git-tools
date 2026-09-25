use gtl_models::failure::ErrorMeta;

#[cfg(test)]
use crate::recipes;
use crate::{
    diffs::{
        DiffTarget, View,
        compute_diff::{self, ComputeDiff},
        compute_merge_diff::{self, ComputeMergeDiff},
    },
    ports::{GitClient, UserSettingsReader},
    recipes::{Recipe, RecipeOp, RecipeTarget},
};

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ComputeRecipeError {
    #[error(transparent)]
    #[meta(transparent)]
    Diff(#[from] compute_diff::ComputeDiffError),
    #[error(transparent)]
    #[meta(transparent)]
    MergeDiff(#[from] compute_merge_diff::ComputeMergeDiffError),
}

#[cqrsy::query]
pub fn execute(
    recipe: Recipe,
    user_settings: &impl UserSettingsReader,
    git: &impl GitClient,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<View, ComputeRecipeError> {
    let cwd = recipe.cwd();
    let view = match recipe.op {
        RecipeOp::Diff { target } => {
            compute_diff::execute(
                ComputeDiff {
                    repo_root: cwd,
                    target: diff_target(target),
                },
                user_settings,
                git,
                comparisons,
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
                git,
            )?
            .view
        }
    };

    Ok(view)
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
            recipe(RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            }),
            &FixedUserSettingsStore::default(),
            &source,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(response.repo_name.as_str(), "project");
        assert_eq!(response.branch.to_string(), "feature");
        assert_eq!(response.upstream.as_ref(), "main");
    }

    #[test]
    fn diff_recipe_targets_preserve_their_ranges() {
        let cases = [
            (
                RecipeTarget::Base {
                    rev: crate::utils::git_revision("v1"),
                },
                gtl_models::diffs::DiffViewTitle::Diff,
                "7631763176",
                "7631763176",
            ),
            (
                RecipeTarget::Range {
                    range: crate::utils::git_range("v1..v2"),
                    pinned: None,
                },
                gtl_models::diffs::DiffViewTitle::Diff,
                "v1..v2",
                "v1..v2",
            ),
            (
                RecipeTarget::Merge {
                    base: crate::utils::git_revision("release"),
                    pinned: None,
                },
                gtl_models::diffs::DiffViewTitle::MergeDiff,
                "release...HEAD",
                "release",
            ),
            (
                RecipeTarget::Last {
                    count: NonZeroU32::new(1).unwrap(),
                    pinned: None,
                },
                gtl_models::diffs::DiffViewTitle::Diff,
                "HEAD~1..HEAD",
                "HEAD~1..HEAD",
            ),
        ];
        let source = source();

        for (target, title, range, upstream) in cases {
            let response = compute_recipe::execute(
                recipe(RecipeOp::Diff { target }),
                &FixedUserSettingsStore::default(),
                &source,
                &crate::utils::ProjectComparisons::default(),
            )
            .unwrap();

            assert_eq!(response.title, title);
            assert_eq!(response.cmd.range, range);
            assert_eq!(response.upstream.as_ref(), upstream);
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
            recipe(RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: crate::utils::git_range("symbolic..range"),
                    pinned: Some(pin()),
                },
            }),
            &FixedUserSettingsStore::default(),
            &source,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(response.cmd.range, "aaaaaaaaaa..1111111111");
    }

    #[test]
    fn recipe_operations_dispatch_to_their_view_queries() {
        let source = FakeGitClient {
            top_level: Some("/repos/project".into()),
            branch: "feature".into(),
            ..Default::default()
        };
        let response = compute_recipe::execute(
            recipe(RecipeOp::MergeDiff {
                base: Some(crate::utils::git_revision("release")),
                pinned: Some(pin()),
            }),
            &FixedUserSettingsStore::default(),
            &source,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(response.title, gtl_models::diffs::DiffViewTitle::MergeDiff);
    }

    #[test]
    fn operation_errors_identify_the_failed_recipe_kind() {
        let source = FakeGitClient {
            top_level: Some("/repos/project".into()),
            branch: "feature".into(),
            ..Default::default()
        };

        let diff = compute_recipe::execute(
            recipe(RecipeOp::Diff {
                target: RecipeTarget::Base {
                    rev: crate::utils::git_revision("unknown"),
                },
            }),
            &FixedUserSettingsStore::default(),
            &source,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap_err();
        let merge = compute_recipe::execute(
            recipe(RecipeOp::MergeDiff {
                base: Some(crate::utils::git_revision("unknown")),
                pinned: None,
            }),
            &FixedUserSettingsStore::default(),
            &source,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap_err();
        assert!(matches!(diff, ComputeRecipeError::Diff(_)));
        assert!(matches!(merge, ComputeRecipeError::MergeDiff(_)));
    }
}
