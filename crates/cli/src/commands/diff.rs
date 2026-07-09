use std::path::{Path, PathBuf};

use anyhow::Context as _;
use contracts::{diffs::RenderDiffRequest, envelope::Outcome};
use gtl_recipe::OpenRecipes;

use crate::{cli::DiffTarget, client::Backend, viewer};

/// Outcome of a single `diff`/`diff merge`/`diff squash` invocation: either an artifact
/// was written, the range was empty and we deliberately skipped rendering a blank
/// preview (`diff` only), or the recipe was forwarded to the viewer app instead (the
/// Phase 5 default — no artifact of our own).
pub enum DiffOutcome {
    Rendered(PathBuf),
    Empty,
    Forwarded,
}

/// Decide whether this invocation takes the `--raw` daemon/store/browser path: either
/// `--raw` was passed explicitly, or there is no display to forward a recipe to (the
/// headless degrade). Pure, so the routing decision is unit-testable without an env var.
pub(crate) fn take_raw_path(raw: bool, has_display: bool) -> bool {
    raw || !has_display
}

/// Render `target`: the `--raw`/headless path renders through the daemon/store/browser;
/// the default path forwards a recipe to the viewer app and degrades to the raw path if
/// the viewer can't be launched (FSD A-0002 — a header-less machine still renders).
pub fn run(target: &DiffTarget, name: Option<&str>, raw: bool) -> anyhow::Result<DiffOutcome> {
    if take_raw_path(raw, viewer::has_display()) {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return render_raw(&backend, target, name);
    }
    let cwd = std::env::current_dir()?;
    render_app(&cwd, target, super::forward_recipes, || {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        render_raw(&backend, target, name)
    })
}

/// Render `target` through `backend`, printing its wire notes and opening the
/// artifact. Split from [`run`] so tests can drive a fake backend. This is the
/// `--raw`/headless path — today's pre-Phase-5 behavior, byte-for-byte.
pub(crate) fn render_raw(
    backend: &impl Backend,
    target: &DiffTarget,
    name: Option<&str>,
) -> anyhow::Result<DiffOutcome> {
    let req = RenderDiffRequest {
        cwd: std::env::current_dir()?.to_string_lossy().into_owned(),
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
        target: super::to_target_dto(target),
        name: name.map(str::to_string),
        theme: crate::config::load().theme,
    };
    let envelope = backend.render_diff(&req)?;
    super::print_wire_notes(&envelope.notes);
    match envelope.outcome {
        Outcome::Ok => {
            let data = envelope.data.context("daemon returned ok without data")?;
            let artifact = PathBuf::from(data.artifact);
            super::open_artifact(&artifact);
            Ok(DiffOutcome::Rendered(artifact))
        }
        Outcome::Empty => Ok(DiffOutcome::Empty),
        Outcome::Error => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
    }
}

/// Resolve `target` (rooted at `cwd`) into a single-recipe batch and hand it to
/// `forward` — the Phase 5 default path. No daemon call, no store artifact on success.
/// If `forward` fails (no viewer binary, or the spawn failed), degrade to `degrade` (the
/// raw path) rather than failing the command — FSD A-0002. A genuine resolution error
/// (e.g. not a git repo) still propagates. Split from [`run`] so tests can drive a fake
/// forwarder + backend instead of spawning the real viewer.
pub(crate) fn render_app(
    cwd: &Path,
    target: &DiffTarget,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
    degrade: impl FnOnce() -> anyhow::Result<DiffOutcome>,
) -> anyhow::Result<DiffOutcome> {
    let op = crate::recipe::diff_op_from_target(target);
    let recipe = crate::recipe::recipe_for_cwd(cwd, op)?;
    let batch = OpenRecipes {
        batch_id: crate::recipe::new_batch_id(),
        recipes: vec![recipe],
    };
    match forward(&batch) {
        Ok(()) => Ok(DiffOutcome::Forwarded),
        Err(err) => {
            super::note_viewer_degrade(&err);
            degrade()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use contracts::{
        diffs::RenderDiffData,
        envelope::{Envelope, Note, NoteLevel, Outcome},
    };
    use gtl_recipe::{RecipeOp, RecipeSource, RecipeTarget};

    use super::*;

    struct FakeBackend(Envelope<RenderDiffData>);

    impl Backend for FakeBackend {
        fn render_diff(
            &self,
            _req: &RenderDiffRequest,
        ) -> anyhow::Result<Envelope<RenderDiffData>> {
            Ok(self.0.clone())
        }
    }

    /// Records whether `render_diff` was invoked, and returns an `Empty` outcome so the
    /// raw path completes without opening anything — lets the degrade tests assert the
    /// raw backend was reached without spawning a viewer or writing an artifact.
    struct TrackingBackend<'a>(&'a Cell<bool>);

    impl Backend for TrackingBackend<'_> {
        fn render_diff(
            &self,
            _req: &RenderDiffRequest,
        ) -> anyhow::Result<Envelope<RenderDiffData>> {
            self.0.set(true);
            Ok(Envelope {
                outcome: Outcome::Empty,
                notes: vec![],
                data: None,
            })
        }
    }

    fn init_repo(dir: &Path) {
        let g = |args: &[&str]| {
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
        g(&["init", "-q"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "first"]);
    }

    #[test]
    fn take_raw_path_is_true_for_explicit_raw_or_no_display() {
        assert!(take_raw_path(true, true));
        assert!(take_raw_path(true, false));
        assert!(take_raw_path(false, false));
        assert!(!take_raw_path(false, true));
    }

    #[test]
    fn render_app_forwards_exactly_one_recipe_matching_the_target_and_calls_no_backend() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let canonical_top = std::fs::canonicalize(tmp.path()).unwrap();
        let captured: RefCell<Option<OpenRecipes>> = RefCell::new(None);

        let outcome = render_app(
            tmp.path(),
            &DiffTarget::Unpushed,
            |batch| {
                *captured.borrow_mut() = Some(batch.clone());
                Ok(())
            },
            || unreachable!("degrade must not run when forwarding succeeds"),
        )
        .unwrap();

        assert!(matches!(outcome, DiffOutcome::Forwarded));
        let batch = captured.into_inner().expect("forward must be called");
        assert_eq!(batch.recipes.len(), 1);
        assert_eq!(
            batch.recipes[0].source,
            RecipeSource::LocalRepo(canonical_top)
        );
        assert_eq!(
            batch.recipes[0].op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed
            }
        );
    }

    #[test]
    fn render_app_degrades_to_the_raw_backend_when_forwarding_fails() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let backend_called = Cell::new(false);
        let backend = TrackingBackend(&backend_called);

        let outcome = render_app(
            tmp.path(),
            &DiffTarget::Unpushed,
            |_batch| anyhow::bail!("gtl-viewer is not installed"),
            || render_raw(&backend, &DiffTarget::Unpushed, None),
        )
        .unwrap();

        assert!(
            backend_called.get(),
            "a forward failure must degrade to the raw backend, not error out"
        );
        assert!(matches!(outcome, DiffOutcome::Empty));
    }

    #[test]
    fn render_app_errors_outside_a_git_repo_without_degrading() {
        let tmp = tempfile::tempdir().unwrap();

        let result = render_app(
            tmp.path(),
            &DiffTarget::Unpushed,
            |_batch| Ok(()),
            || unreachable!("a genuine resolution error must propagate, not degrade"),
        );

        assert!(result.is_err());
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
        let Err(err) = render_raw(&backend, &DiffTarget::Unpushed, None) else {
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
            render_raw(&backend, &DiffTarget::Unpushed, None).unwrap(),
            DiffOutcome::Empty
        ));
    }
}
