use std::path::PathBuf;

use application::diffs::render_diff::{RenderDiff, RenderDiffHandler, RenderDiffOutcome};

use crate::cli::DiffTarget;

/// Outcome of a single `diff` invocation: either an artifact was written, or the
/// range was empty and we deliberately skipped rendering a blank preview.
pub enum DiffOutcome {
    Rendered(PathBuf),
    Empty,
}

pub fn run(target: &DiffTarget, name: Option<&str>) -> anyhow::Result<DiffOutcome> {
    let handler = RenderDiffHandler {
        source: infra::diff_source::GitDiffSource,
        store: infra::artifact_store::StoreArtifacts,
        renderer: infra::html_renderer::MaudRenderer,
        clock: infra::clock::SystemClock,
    };
    let req = RenderDiff {
        cwd: std::env::current_dir()?,
        store_root: gtl_platform::paths::store_root()?,
        target: target.clone(),
        name: name.map(str::to_string),
        theme: crate::config::load().theme,
    };
    let response = handler.execute(&req).map_err(anyhow::Error::from)?;
    super::print_notes(&response.notes);
    match response.outcome {
        RenderDiffOutcome::Rendered { artifact, .. } => {
            super::open_artifact(&artifact);
            Ok(DiffOutcome::Rendered(artifact))
        }
        RenderDiffOutcome::Empty => Ok(DiffOutcome::Empty),
    }
}
