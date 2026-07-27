use contracts::recipes::{Recipe, RecipeOp, RecipeTarget};

use crate::diffs::{View, render_merge_diff::DEFAULT_BASE};

pub(super) fn initial(recipe: &Recipe) -> String {
    if let Some(name) = &recipe.name {
        return name.clone();
    }

    let repo = source_repo_name(recipe);
    match &recipe.op {
        RecipeOp::Diff { target } => initial_diff_label(&repo, target),
        RecipeOp::MergeDiff { base, .. } => {
            let base = base
                .as_deref()
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(DEFAULT_BASE);
            format!("{repo}: merge ->{base}")
        }
        RecipeOp::SquashPreview { .. } => format!("{repo}: squash"),
    }
}

pub(super) fn computed(recipe: &Recipe, view: &View) -> String {
    if let Some(name) = &recipe.name {
        return name.clone();
    }

    let repo = &view.repo_name;
    match &recipe.op {
        RecipeOp::Diff { target } => computed_diff_label(repo, target, view),
        RecipeOp::MergeDiff { .. } => merge_label(repo, view),
        RecipeOp::SquashPreview { .. } => {
            format!("{repo}: squash {}", commit_count(view.commits.len()))
        }
    }
}

fn initial_diff_label(repo: &str, target: &RecipeTarget) -> String {
    match target {
        RecipeTarget::Unpushed { .. } => format!("{repo}: diff"),
        RecipeTarget::Base { rev } => format!("{repo}: {rev}->working"),
        RecipeTarget::Range { range, .. } => format!("{repo}: {range}"),
        RecipeTarget::Merge { base, .. } => {
            let base = base.trim();
            if base.is_empty() {
                format!("{repo}: merge")
            } else {
                format!("{repo}: merge ->{base}")
            }
        }
        RecipeTarget::Last { count, .. } => {
            format!("{repo}: last {}", commit_count(count.get() as usize))
        }
    }
}

fn computed_diff_label(repo: &str, target: &RecipeTarget, view: &View) -> String {
    match target {
        RecipeTarget::Unpushed { .. } => {
            format!("{repo}: {}", commit_count(view.commits.len()))
        }
        RecipeTarget::Base { rev } => format!("{repo}: {rev}->working"),
        RecipeTarget::Range { range, .. } => format!("{repo}: {range}"),
        RecipeTarget::Merge { .. } => merge_label(repo, view),
        RecipeTarget::Last { count, .. } => {
            format!("{repo}: last {}", commit_count(count.get() as usize))
        }
    }
}

fn merge_label(repo: &str, view: &View) -> String {
    format!("{repo}: merge {}->{}", view.branch, view.upstream)
}

fn commit_count(count: usize) -> String {
    let suffix = if count == 1 { "commit" } else { "commits" };
    format!("{count} {suffix}")
}

fn source_repo_name(recipe: &Recipe) -> String {
    let cwd = recipe.cwd();
    cwd.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| cwd.display().to_string())
}
