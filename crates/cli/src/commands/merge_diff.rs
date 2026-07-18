use std::path::Path;

use application::{diffs::render_merge_diff::RenderMergeDiff, recipes::RecipeRequest};
use gtl_recipe::{OpenRecipes, RecipeBatchKind};

use crate::{
    client::Backend,
    commands::diff::{DiffOutcome, take_raw_path},
    viewer,
};

/// Render `repo`'s merge-diff: raw/headless invocations use the daemon/store/browser;
/// displayed app-default invocations forward a recipe and degrade to that raw path when
/// the viewer is unavailable.
pub fn run(repo: impl AsRef<Path>, base: Option<&str>, raw: bool) -> anyhow::Result<DiffOutcome> {
    if take_raw_path(raw, viewer::has_display()) {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return render(&backend, repo, base, super::open_artifact);
    }
    if viewer::no_open_requested() {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return render(&backend, repo, base, super::do_not_open);
    }
    let repo = repo.as_ref();
    render_app(repo, base, super::forward_recipes, || {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        render(&backend, repo, base, super::open_artifact)
    })
}

fn render_app(
    repo: &Path,
    base: Option<&str>,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
    degrade: impl FnOnce() -> anyhow::Result<DiffOutcome>,
) -> anyhow::Result<DiffOutcome> {
    let recipe = crate::recipe::recipe_for_cwd(
        repo,
        RecipeRequest::MergeDiff {
            base: base.map(str::to_string),
        },
        None,
    )?;
    let batch = OpenRecipes {
        batch_id: crate::recipe::new_batch_id(),
        kind: RecipeBatchKind::Snapshot,
        recipes: vec![recipe],
    };
    match forward(&batch) {
        Ok(()) => Ok(DiffOutcome::Forwarded),
        Err(error) => {
            super::note_viewer_degrade(&error);
            degrade()
        }
    }
}

/// Render `repo`'s merge-diff into `base` through `backend`, printing its wire notes and
/// handing the artifact to `open`. Split from [`run`] so tests can drive a fake backend.
pub(crate) fn render(
    backend: &impl Backend,
    repo: impl AsRef<Path>,
    base: Option<&str>,
    open: impl FnOnce(&Path),
) -> anyhow::Result<DiffOutcome> {
    // Lexical, no filesystem access — the daemon process's own cwd is unrelated
    // to the caller's shell, so `cwd` must already be absolute on the wire.
    let cwd = std::path::absolute(repo.as_ref())?;
    let request = RenderMergeDiff {
        cwd,
        store_root: gtl_platform::paths::store_root()?,
        base: base.map(str::to_string),
    };
    let artifact = super::finish_single_render(backend.render_merge_diff(&request)?, open)?;
    Ok(DiffOutcome::Rendered(artifact))
}

#[cfg(test)]
mod tests {
    use contracts::{
        diffs::RenderDiffData,
        envelope::{Envelope, Note, NoteLevel, Outcome},
    };

    use super::*;

    struct FakeBackend(Envelope<RenderDiffData>);

    impl Backend for FakeBackend {
        fn render_merge_diff(
            &self,
            _request: &RenderMergeDiff,
        ) -> anyhow::Result<Envelope<RenderDiffData>> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn error_outcome_surfaces_the_error_note_text() {
        let backend = FakeBackend(Envelope {
            outcome: Outcome::Error,
            notes: vec![Note {
                level: NoteLevel::Error,
                text: "not a git repo".into(),
            }],
            data: None,
        });
        let Err(err) = render(&backend, ".", None, |_| {}) else {
            panic!("error outcome must map to Err")
        };
        assert_eq!(format!("{err:#}"), "not a git repo");
    }
}
