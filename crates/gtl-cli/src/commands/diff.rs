use gtl_application::{
    diffs::{
        present_diff::{DiffRecipeIntent, PresentDiff},
        render_diff::RenderDiff,
    },
    ports::{DiffRenderRequest, GitClient as _},
};
use gtl_models::paths::{AbsoluteFilePath, ProjectName};

use crate::{cli::DiffTarget, recipe};

pub enum DiffOutcome {
    Rendered(AbsoluteFilePath),
    Empty,
    Forwarded,
}

pub fn run(target: &DiffTarget, name: Option<&str>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let cwd = std::env::current_dir()?;
    let repo_root = gtl_infra::git_client::HybridGitClient.top_level(&cwd)?;
    super::present(PresentDiff {
        render: DiffRenderRequest::Diff(RenderDiff {
            cwd: cwd.clone(),
            target: target.into(),
            name: name.map(str::to_string),
        }),
        batch_id: crate::recipe::new_batch_id(),
        recipes: vec![DiffRecipeIntent {
            repo_root,
            operation: recipe::diff_operation(target),
            name: name.map(ProjectName::try_from).transpose()?,
        }],
        mode: super::presentation_mode(raw),
    })
}
