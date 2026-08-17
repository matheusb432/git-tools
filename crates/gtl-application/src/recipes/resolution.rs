//! Shared symbolic-range resolution for recipe-building interactors.

use gtl_models::{
    git::GitRevision,
    paths::{ProjectName, RepositoryRoot},
};
use gtl_wire::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};

use crate::{ports::GitClient, shared::git_range_pinning};

pub(crate) fn build_resolved(
    repo_top: RepositoryRoot,
    operation: RecipeOp,
    name: Option<ProjectName>,
    git: &impl GitClient,
) -> Recipe {
    Recipe {
        op: pin_operation(&repo_top, operation, git),
        source: RecipeSource::LocalRepo(repo_top),
        name,
    }
}

fn pin_operation(repo_top: &RepositoryRoot, operation: RecipeOp, git: &impl GitClient) -> RecipeOp {
    match operation {
        RecipeOp::Diff { target } => RecipeOp::Diff {
            target: pin_target(repo_top, target, git),
        },
        RecipeOp::MergeDiff { base, pinned: None } => RecipeOp::MergeDiff {
            pinned: git_range_pinning::resolve_merge_range(repo_top, base.as_ref(), git),
            base,
        },
        operation @ RecipeOp::MergeDiff {
            pinned: Some(_), ..
        } => operation,
    }
}

fn pin_target(
    repo_top: &RepositoryRoot,
    target: RecipeTarget,
    git: &impl GitClient,
) -> RecipeTarget {
    match target {
        RecipeTarget::Unpushed { pinned: None } => RecipeTarget::Unpushed {
            pinned: git_range_pinning::resolve_range(
                repo_top,
                &GitRevision::upstream(),
                &GitRevision::head(),
                git,
            ),
        },
        RecipeTarget::Last {
            count,
            pinned: None,
        } => RecipeTarget::Last {
            count,
            pinned: git_range_pinning::resolve_range(
                repo_top,
                &GitRevision::head_ancestor(count),
                &GitRevision::head(),
                git,
            ),
        },
        RecipeTarget::Range {
            range,
            pinned: None,
        } => RecipeTarget::Range {
            pinned: git_range_pinning::resolve_exact_range(repo_top, &range, git),
            range,
        },
        RecipeTarget::Merge { base, pinned: None } => RecipeTarget::Merge {
            pinned: git_range_pinning::resolve_merge_range(repo_top, Some(&base), git),
            base,
        },
        target => target,
    }
}
