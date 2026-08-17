use std::path::Path;

use gtl_application::{
    diffs::{
        present_diff::{DiffRecipeIntent, PresentDiff},
        render_merge_diff::RenderMergeDiff,
    },
    ports::{DiffRenderRequest, GitClient as _},
};
use gtl_models::git::GitRevision;
use gtl_wire::recipes::RecipeOp;

use crate::commands::diff::DiffOutcome;

pub fn run(
    repo_path: impl AsRef<Path>,
    base: Option<&str>,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    let cwd = std::path::absolute(repo_path.as_ref())?;
    let repo_root = gtl_infra::git_client::HybridGitClient.top_level(&cwd)?;
    let base = base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .map(|base| GitRevision::try_new(base.to_owned()))
        .transpose()
        .map_err(|_| anyhow::anyhow!("merge base must not be empty"))?;
    super::present(PresentDiff {
        render: DiffRenderRequest::MergeDiff(RenderMergeDiff {
            cwd: cwd.clone(),
            base: base.clone(),
        }),
        batch_id: crate::recipe::new_batch_id(),
        recipes: vec![DiffRecipeIntent {
            repo_root,
            operation: RecipeOp::MergeDiff { base, pinned: None },
            name: None,
        }],
        mode: super::presentation_mode(raw),
    })
}
