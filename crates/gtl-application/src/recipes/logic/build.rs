use std::path::Path;

use gtl_contracts::recipes::{PinnedRange, Recipe, RecipeOp, RecipeSource, RecipeTarget};

use crate::{
    diffs::{self, DiffTarget},
    ports::GitClient,
    recipes::RecipeRequest,
    shared::git_range_pinning,
};

pub(crate) fn build_resolved(
    repo_top: impl Into<std::path::PathBuf>,
    operation: RecipeRequest,
    name: Option<String>,
    git: &impl GitClient,
) -> Recipe {
    let repo_top = repo_top.into();
    Recipe {
        op: pin_operation(&repo_top, operation_to_op(operation), git),
        source: RecipeSource::LocalRepo(repo_top),
        name,
    }
}

fn operation_to_op(operation: RecipeRequest) -> RecipeOp {
    match operation {
        RecipeRequest::Diff(target) => RecipeOp::Diff {
            target: target_to_recipe(target),
        },
        RecipeRequest::MergeDiff { base } => RecipeOp::MergeDiff { base, pinned: None },
    }
}

fn target_to_recipe(target: DiffTarget) -> RecipeTarget {
    match target {
        DiffTarget::Unpushed { pinned } => RecipeTarget::Unpushed {
            pinned: pinned.map(pin_to_recipe),
        },
        DiffTarget::Base(rev) => RecipeTarget::Base { rev },
        DiffTarget::Range { range, pinned } => RecipeTarget::Range {
            range,
            pinned: pinned.map(pin_to_recipe),
        },
        DiffTarget::Merge { base, pinned } => RecipeTarget::Merge {
            base,
            pinned: pinned.map(pin_to_recipe),
        },
        DiffTarget::Last { count, pinned } => RecipeTarget::Last {
            count,
            pinned: pinned.map(pin_to_recipe),
        },
    }
}

fn pin_to_recipe(pin: diffs::PinnedRange) -> PinnedRange {
    PinnedRange {
        base: pin.base,
        head: pin.head,
    }
}

fn resolved_pin_to_recipe(pin: git_range_pinning::ResolvedGitRange) -> PinnedRange {
    PinnedRange {
        base: pin.base,
        head: pin.head,
    }
}

fn pin_operation(repo_top: &Path, operation: RecipeOp, git: &impl GitClient) -> RecipeOp {
    match operation {
        RecipeOp::Diff { target } => RecipeOp::Diff {
            target: pin_target(repo_top, target, git),
        },
        RecipeOp::MergeDiff { base, pinned: None } => RecipeOp::MergeDiff {
            pinned: git_range_pinning::resolve_merge_range(repo_top, base.as_deref(), git)
                .map(resolved_pin_to_recipe),
            base,
        },
        operation @ RecipeOp::MergeDiff {
            pinned: Some(_), ..
        } => operation,
    }
}

fn pin_target(repo_top: &Path, target: RecipeTarget, git: &impl GitClient) -> RecipeTarget {
    match target {
        RecipeTarget::Unpushed { pinned: None } => RecipeTarget::Unpushed {
            pinned: git_range_pinning::resolve_range(repo_top, "@{u}", "HEAD", git)
                .map(resolved_pin_to_recipe),
        },
        RecipeTarget::Last {
            count,
            pinned: None,
        } => RecipeTarget::Last {
            count,
            pinned: git_range_pinning::resolve_range(
                repo_top,
                &format!("HEAD~{count}"),
                "HEAD",
                git,
            )
            .map(resolved_pin_to_recipe),
        },
        RecipeTarget::Range {
            range,
            pinned: None,
        } => RecipeTarget::Range {
            pinned: git_range_pinning::resolve_exact_range(repo_top, &range, git)
                .map(resolved_pin_to_recipe),
            range,
        },
        RecipeTarget::Merge { base, pinned: None } => RecipeTarget::Merge {
            pinned: git_range_pinning::resolve_merge_range(repo_top, Some(&base), git)
                .map(resolved_pin_to_recipe),
            base,
        },
        target => target,
    }
}
