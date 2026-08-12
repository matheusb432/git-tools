use gtl_application::{
    diffs::{
        present_diff::{DiffRecipeIntent, PresentDiff},
        render_diff::RenderDiff,
    },
    ports::DiffRenderRequest,
};

use crate::{cli::DiffTarget, recipe, viewer};

pub enum DiffOutcome {
    Rendered(std::path::PathBuf),
    Empty,
    Forwarded,
}

pub fn run(target: &DiffTarget, name: Option<&str>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let cwd = std::env::current_dir()?;
    super::present(PresentDiff {
        render: DiffRenderRequest::Diff(RenderDiff {
            cwd: cwd.clone(),
            target: target.into(),
            name: name.map(str::to_string),
        }),
        batch_id: crate::recipe::new_batch_id(),
        recipes: vec![DiffRecipeIntent {
            repo_path: cwd,
            operation: recipe::diff_operation(target),
            name: name.map(str::to_string),
        }],
        raw,
        has_display: viewer::has_display(),
        effects_enabled: !viewer::no_open_requested(),
    })
}
