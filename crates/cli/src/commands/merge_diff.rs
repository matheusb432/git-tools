use std::path::Path;

use application::{
    diffs::{
        present_diff::{DiffRecipeIntent, PresentDiff},
        render_merge_diff::RenderMergeDiff,
    },
    ports::DiffRenderRequest,
    recipes::RecipeRequest,
};

use crate::{commands::diff::DiffOutcome, viewer};

pub fn run(repo: impl AsRef<Path>, base: Option<&str>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let cwd = std::path::absolute(repo.as_ref())?;
    super::present(PresentDiff {
        render: DiffRenderRequest::MergeDiff(RenderMergeDiff {
            cwd: cwd.clone(),
            base: base.map(str::to_string),
        }),
        batch_id: crate::recipe::new_batch_id(),
        recipes: vec![DiffRecipeIntent {
            repo: cwd,
            operation: RecipeRequest::MergeDiff {
                base: base.map(str::to_string),
            },
            name: None,
        }],
        raw,
        has_display: viewer::has_display(),
        effects_enabled: !viewer::no_open_requested(),
    })
}
