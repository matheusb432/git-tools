use std::path::Path;

use contracts::diffs::RenderMergeDiffRequest;
use gtl_recipe::{OpenRecipes, RecipeBatchKind, RecipeOp};

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
        RecipeOp::MergeDiff {
            base: base.map(str::to_string),
            pinned: None,
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
    let req = RenderMergeDiffRequest {
        cwd: cwd.to_string_lossy().into_owned(),
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
        base: base.map(str::to_string),
    };
    let artifact = super::finish_single_render(backend.render_merge_diff(&req)?, open)?;
    Ok(DiffOutcome::Rendered(artifact))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use contracts::{
        diffs::RenderDiffData,
        envelope::{Envelope, Note, NoteLevel, Outcome},
    };

    use super::*;

    struct FakeBackend(Envelope<RenderDiffData>);

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

    impl Backend for FakeBackend {
        fn render_merge_diff(
            &self,
            _req: &RenderMergeDiffRequest,
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

    #[test]
    fn app_path_forwards_the_merge_recipe() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        // `-M` guarantees a `main` ref regardless of the ambient init.defaultBranch
        // config (ordinary `git branch -f main` fails when `main` is already the
        // checked-out branch, and the default branch name can otherwise race with
        // other tests that mutate the process-global `HOME` env var).
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(["branch", "-M", "main"])
                .status()
                .unwrap()
                .success()
        );
        // Single commit on `main` — merge-base against itself resolves both pin
        // endpoints to that one commit's sha (GTL-0131: recipe_for_cwd pins at mint
        // time via pin_op).
        let head = std::process::Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        let sha = String::from_utf8(head.stdout).unwrap().trim().to_string();
        let captured = RefCell::new(None);

        let outcome = render_app(
            repo.path(),
            Some("main"),
            |batch| {
                *captured.borrow_mut() = Some(batch.clone());
                Ok(())
            },
            || unreachable!("successful forwarding must not degrade"),
        )
        .unwrap();

        assert!(matches!(outcome, DiffOutcome::Forwarded));
        assert_eq!(
            captured.into_inner().unwrap().recipes[0].op,
            RecipeOp::MergeDiff {
                base: Some("main".into()),
                pinned: Some(gtl_recipe::PinnedRange {
                    base: sha.clone(),
                    head: sha
                })
            }
        );
    }
}
