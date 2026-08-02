use std::path::Path;

use gtl_application::{
    diffs::{
        present_diff::{DiffRecipeIntent, PresentDiff},
        render_squash_preview::RenderSquashPreview,
    },
    ports::DiffRenderRequest,
    recipes::RecipeRequest,
};

use crate::{commands::diff::DiffOutcome, viewer};

pub fn run(repo: impl AsRef<Path>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let cwd = std::path::absolute(repo.as_ref())?;
    super::present(PresentDiff {
        render: DiffRenderRequest::SquashPreview(RenderSquashPreview { cwd: cwd.clone() }),
        batch_id: crate::recipe::new_batch_id(),
        recipes: vec![DiffRecipeIntent {
            repo: cwd,
            operation: RecipeRequest::SquashPreview,
            name: None,
        }],
        raw,
        has_display: viewer::has_display(),
        effects_enabled: !viewer::no_open_requested(),
    })
}
