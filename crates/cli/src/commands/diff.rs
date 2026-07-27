use std::path::{Path, PathBuf};

use anyhow::Context as _;
use application::{diffs::render_diff::RenderDiff, recipes::RecipeRequest};
use contracts::{
    envelope::Outcome,
    recipes::{OpenRecipes, RecipeBatchKind},
};

use crate::{cli::DiffTarget, client::Backend, viewer};

/// Outcome of a single `diff`/`diff merge`/`diff squash` invocation: either an artifact
/// was written, or the range was empty and we deliberately skipped rendering a blank
/// preview (`diff` only).
pub enum DiffOutcome {
    Rendered(PathBuf),
    Empty,
    Forwarded,
}

/// Decide whether this invocation opens the artifact in the browser (`--raw`) rather
/// than the desktop viewer: either `--raw` was passed explicitly, or there is no display
/// to spawn a viewer on (the headless degrade). Pure, so the routing decision is
/// unit-testable without an env var.
pub(crate) fn take_raw_path(raw: bool, has_display: bool) -> bool {
    raw || !has_display
}

/// Render `target`: raw/headless invocations use the daemon/store/browser path;
/// displayed app-default invocations forward a recipe and use that raw path only when
/// the viewer is unavailable. `GIT_TOOLS_NO_OPEN` preserves the daemon/store path while
/// suppressing both viewer and browser effects.
pub fn run(target: &DiffTarget, name: Option<&str>, raw: bool) -> anyhow::Result<DiffOutcome> {
    if take_raw_path(raw, viewer::has_display()) {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return render(&backend, target, name, super::open_artifact);
    }
    if viewer::no_open_requested() {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return render(&backend, target, name, super::do_not_open);
    }
    let cwd = std::env::current_dir()?;
    render_app(&cwd, target, name, super::forward_recipes, || {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        render(&backend, target, name, super::open_artifact)
    })
}

pub(crate) fn render_app(
    cwd: &Path,
    target: &DiffTarget,
    name: Option<&str>,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
    degrade: impl FnOnce() -> anyhow::Result<DiffOutcome>,
) -> anyhow::Result<DiffOutcome> {
    let recipe = crate::recipe::recipe_for_cwd(cwd, RecipeRequest::Diff(target.clone()), name)?;
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

/// Render `target` through `backend`, printing its wire notes and handing the artifact
/// to `open`. Split from [`run`] so tests can drive a fake backend.
pub(crate) fn render(
    backend: &impl Backend,
    target: &DiffTarget,
    name: Option<&str>,
    open: impl FnOnce(&Path),
) -> anyhow::Result<DiffOutcome> {
    let request = RenderDiff {
        cwd: std::env::current_dir()?,
        store_root: gtl_platform::paths::store_root()?,
        target: target.into(),
        name: name.map(str::to_string),
    };
    let envelope = backend.render_diff(&request)?;
    super::print_wire_notes(&envelope.notes);
    match envelope.outcome {
        Outcome::Ok => {
            let data = envelope.data.context("daemon returned ok without data")?;
            let artifact = PathBuf::from(data.artifact);
            open(&artifact);
            Ok(DiffOutcome::Rendered(artifact))
        }
        Outcome::Empty => Ok(DiffOutcome::Empty),
        Outcome::Error => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use contracts::{
        diffs::RenderDiffData,
        envelope::{Envelope, Note, NoteLevel, Outcome},
    };

    use super::*;

    fn init_repo(dir: &Path) {
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(dir)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "test@example.invalid"]);
        git(&["config", "user.name", "Test"]);
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "first"]);
    }

    struct FakeBackend(Envelope<RenderDiffData>);

    impl Backend for FakeBackend {
        fn render_diff(&self, _request: &RenderDiff) -> anyhow::Result<Envelope<RenderDiffData>> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn take_raw_path_is_true_for_explicit_raw_or_no_display() {
        assert!(take_raw_path(true, true));
        assert!(take_raw_path(true, false));
        assert!(take_raw_path(false, false));
        assert!(!take_raw_path(false, true));
    }

    #[test]
    fn app_path_forwards_one_named_recipe_without_degrading() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        let captured = RefCell::new(None);

        let outcome = render_app(
            repo.path(),
            &DiffTarget::Unpushed { pinned: None },
            Some("release review"),
            |batch| {
                *captured.borrow_mut() = Some(batch.clone());
                Ok(())
            },
            || unreachable!("a successful forward must not enter daemon degradation"),
        )
        .unwrap();

        assert!(matches!(outcome, DiffOutcome::Forwarded));
        let batch = captured.into_inner().expect("a recipe batch");
        assert_eq!(batch.kind, RecipeBatchKind::Snapshot);
        assert_eq!(batch.recipes.len(), 1);
        assert_eq!(batch.recipes[0].name.as_deref(), Some("release review"));
        assert_eq!(
            batch.recipes[0].source,
            contracts::recipes::RecipeSource::LocalRepo(
                std::fs::canonicalize(repo.path()).unwrap()
            )
        );
    }

    #[test]
    fn app_path_enters_daemon_degradation_only_when_forwarding_fails() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        let degraded = Cell::new(false);

        let outcome = render_app(
            repo.path(),
            &DiffTarget::Unpushed { pinned: None },
            None,
            |_batch| anyhow::bail!("viewer unavailable"),
            || {
                degraded.set(true);
                Ok(DiffOutcome::Empty)
            },
        )
        .unwrap();

        assert!(degraded.get());
        assert!(matches!(outcome, DiffOutcome::Empty));
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
        let Err(err) = render(
            &backend,
            &DiffTarget::Unpushed { pinned: None },
            None,
            |_| {},
        ) else {
            panic!("error outcome must map to Err")
        };
        assert_eq!(format!("{err:#}"), "not a git repo");
    }

    #[test]
    fn empty_outcome_maps_to_empty() {
        let backend = FakeBackend(Envelope {
            outcome: Outcome::Empty,
            notes: vec![],
            data: None,
        });
        assert!(matches!(
            render(
                &backend,
                &DiffTarget::Unpushed { pinned: None },
                None,
                |_| {}
            )
            .unwrap(),
            DiffOutcome::Empty
        ));
    }
}
