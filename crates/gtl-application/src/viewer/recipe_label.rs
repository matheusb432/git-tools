use gtl_models::paths::ProjectName;

use crate::{
    diffs::render_merge_diff::DEFAULT_BASE,
    recipes::{Recipe, RecipeOp, RecipeTarget},
};

pub(super) fn initial(recipe: &Recipe) -> String {
    if let Some(name) = &recipe.name {
        return name.to_string();
    }

    let repo = source_repo_name(recipe);
    match &recipe.op {
        RecipeOp::Diff { target } => initial_diff_label(&repo, target),
        RecipeOp::MergeDiff { base, .. } => {
            let base = base.as_ref().map_or(DEFAULT_BASE, AsRef::as_ref);
            format!("{repo}: merge ->{base}")
        }
    }
}

fn initial_diff_label(repo: &str, target: &RecipeTarget) -> String {
    match target {
        RecipeTarget::Unpushed { .. } => format!("{repo}: diff"),
        RecipeTarget::Base { rev } => format!("{repo}: {rev}->working"),
        RecipeTarget::Range { range, .. } => format!("{repo}: {range}"),
        RecipeTarget::Merge { base, .. } => format!("{repo}: merge ->{base}"),
        RecipeTarget::Last { count, .. } => {
            format!("{repo}: last {}", commit_count(count.get() as usize))
        }
    }
}

pub(super) fn commit_count(count: usize) -> String {
    let suffix = if count == 1 { "commit" } else { "commits" };
    format!("{count} {suffix}")
}

fn source_repo_name(recipe: &Recipe) -> ProjectName {
    recipe.cwd().project_name()
}
