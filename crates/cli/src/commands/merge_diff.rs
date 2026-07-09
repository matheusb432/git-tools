use std::path::Path;

use contracts::diffs::RenderMergeDiffRequest;
use gtl_recipe::{OpenRecipes, RecipeOp};

use crate::{
    client::Backend,
    commands::diff::{DiffOutcome, take_raw_path},
    viewer,
};

/// Render `repo`'s merge-diff into `base`: the `--raw`/headless path renders through the
/// daemon/store/browser; the default path forwards a recipe to the viewer app and
/// degrades to the raw path if the viewer can't be launched (FSD A-0002 — a header-less
/// machine still renders).
pub fn run(repo: impl AsRef<Path>, base: Option<&str>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let repo = repo.as_ref();
    if take_raw_path(raw, viewer::has_display()) {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return render_raw(&backend, repo, base);
    }
    // Lexical, no filesystem access — matches `render_raw`'s own `cwd` resolution.
    let cwd = std::path::absolute(repo)?;
    render_app(&cwd, base, super::forward_recipes, || {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        render_raw(&backend, repo, base)
    })
}

/// Render `repo`'s merge-diff into `base` through `backend`, printing its wire notes and
/// opening the artifact. Split from [`run`] so tests can drive a fake backend. This is
/// the `--raw`/headless path — today's pre-Phase-5 behavior, byte-for-byte.
pub(crate) fn render_raw(
    backend: &impl Backend,
    repo: impl AsRef<Path>,
    base: Option<&str>,
) -> anyhow::Result<DiffOutcome> {
    // Lexical, no filesystem access — the daemon process's own cwd is unrelated
    // to the caller's shell, so `cwd` must already be absolute on the wire.
    let cwd = std::path::absolute(repo.as_ref())?;
    let req = RenderMergeDiffRequest {
        cwd: cwd.to_string_lossy().into_owned(),
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
        base: base.map(str::to_string),
    };
    let artifact = super::finish_single_render(backend.render_merge_diff(&req)?)?;
    Ok(DiffOutcome::Rendered(artifact))
}

/// Resolve `cwd` into a single-recipe `MergeDiff` batch and hand it to `forward` — the
/// Phase 5 default path. No daemon call, no store artifact on success. If `forward`
/// fails (no viewer binary, or the spawn failed), degrade to `degrade` (the raw path)
/// rather than failing the command — FSD A-0002. A genuine resolution error (e.g. not a
/// git repo) still propagates. Split from [`run`] so tests can drive a fake forwarder +
/// backend instead of spawning the real viewer.
pub(crate) fn render_app(
    cwd: &Path,
    base: Option<&str>,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
    degrade: impl FnOnce() -> anyhow::Result<DiffOutcome>,
) -> anyhow::Result<DiffOutcome> {
    let op = RecipeOp::MergeDiff {
        base: base.map(str::to_string),
    };
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
    use gtl_recipe::RecipeSource;

    use super::*;

    struct FakeBackend(Envelope<RenderDiffData>);

    impl Backend for FakeBackend {
        fn render_merge_diff(
            &self,
            _req: &RenderMergeDiffRequest,
        ) -> anyhow::Result<Envelope<RenderDiffData>> {
            Ok(self.0.clone())
        }
    }

    /// Records whether `render_merge_diff` was invoked, and returns an `Ok` outcome with
    /// a placeholder artifact so the degrade tests can assert the raw backend was
    /// reached without spawning a real viewer.
    struct TrackingBackend<'a>(&'a Cell<bool>);

    impl Backend for TrackingBackend<'_> {
        fn render_merge_diff(
            &self,
            _req: &RenderMergeDiffRequest,
        ) -> anyhow::Result<Envelope<RenderDiffData>> {
            self.0.set(true);
            Ok(Envelope {
                outcome: Outcome::Ok,
                notes: vec![],
                data: Some(RenderDiffData {
                    artifact: "/tmp/fake.html".to_string(),
                    reused: false,
                }),
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
    fn render_app_forwards_exactly_one_recipe_matching_the_base_and_calls_no_backend() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let canonical_top = std::fs::canonicalize(tmp.path()).unwrap();
        let captured: RefCell<Option<OpenRecipes>> = RefCell::new(None);

        let outcome = render_app(
            tmp.path(),
            Some("main"),
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
            RecipeOp::MergeDiff {
                base: Some("main".to_string())
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
            None,
            |_batch| anyhow::bail!("gtl-viewer is not installed"),
            || render_raw(&backend, tmp.path(), None),
        )
        .unwrap();

        assert!(
            backend_called.get(),
            "a forward failure must degrade to the raw backend, not error out"
        );
        assert!(matches!(outcome, DiffOutcome::Rendered(_)));
    }

    #[test]
    fn render_app_errors_outside_a_git_repo_without_degrading() {
        let tmp = tempfile::tempdir().unwrap();

        let result = render_app(
            tmp.path(),
            None,
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
        let Err(err) = render_raw(&backend, ".", None) else {
            panic!("error outcome must map to Err")
        };
        assert_eq!(format!("{err:#}"), "not a git repo");
    }
}
