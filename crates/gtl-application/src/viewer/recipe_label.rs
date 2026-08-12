use gtl_wire::recipes::{Recipe, RecipeOp, RecipeTarget};

use crate::diffs::render_merge_diff::DEFAULT_BASE;

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

pub(super) fn commit_count(count: usize) -> String {
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
