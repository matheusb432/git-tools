use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use anyhow::Context;
use contracts::{
    diffs::{RenderDiffAllRequest, RenderDiffSubreposRequest, RepoRefDto},
    envelope::Outcome,
};
use gtl_recipe::OpenRecipes;

use crate::{
    cli::DiffTarget,
    client::Backend,
    commands::{
        diff::{DiffOutcome, take_raw_path},
        discover::{discover_git_repos, repo_label},
        managed::{self, ManagedOptions},
    },
    git, recipe, viewer,
};

/// `diff -r`: default forwards one recipe batch to the viewer (no daemon, no
/// artifact); `--raw`, or no display, keeps the daemon/store/browser path
/// ([`run_scan_with`]).
pub fn run_scan(
    root: impl AsRef<Path>,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    if take_raw_path(raw, viewer::has_display()) {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return run_scan_with(&backend, root, last, include_worktrees);
    }
    let root = std::fs::canonicalize(root.as_ref())
        .with_context(|| format!("failed to resolve {}", root.as_ref().display()))?;
    forward_scan(
        &root,
        last,
        include_worktrees,
        super::forward_recipes,
        || {
            let backend = crate::client::HttpBackend::ensure_daemon()?;
            run_scan_with(&backend, &root, last, include_worktrees)
        },
    )
}

/// Resolve `root`'s repo scan into a recipe batch and hand it to `forward`. On a forward
/// failure (no viewer binary, or spawn failed) degrade to `degrade` (the raw path) rather
/// than failing — FSD A-0002. A genuine error (no repos found, discovery failure) still
/// propagates. Split from [`run_scan`] so tests can drive a fake forwarder + backend
/// instead of spawning the real viewer.
pub(crate) fn forward_scan(
    root: &Path,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
    degrade: impl FnOnce() -> anyhow::Result<DiffOutcome>,
) -> anyhow::Result<DiffOutcome> {
    let recipes = recipe::subrepo_recipes(root, last, include_worktrees)?;
    if recipes.is_empty() {
        anyhow::bail!("diff -r: no git repos found under {}", root.display());
    }
    let batch = OpenRecipes {
        batch_id: recipe::new_batch_id(),
        recipes,
    };
    match forward(&batch) {
        Ok(()) => Ok(DiffOutcome::Forwarded),
        Err(err) => {
            super::note_viewer_degrade(&err);
            degrade()
        }
    }
}

/// A discovered or pre-filtered repo, resolved to its canonical top-level path and a
/// display label. The unit shared by both the raw `RepoRefDto` wire path
/// ([`run_scan_with`], [`run_managed_all_with`]) and the recipe path
/// (`crate::recipe::subrepo_recipes`, `crate::recipe::managed_recipes`), so discovery
/// and pre-filtering are implemented exactly once.
pub(crate) struct RepoTop {
    pub top: PathBuf,
    pub label: String,
}

/// Walk `root` for git repos (see [`discover_git_repos`]) and resolve each to its
/// canonical top-level path + a label relative to `root`. No unpushed filtering —
/// `diff -r` renders every discovered repo regardless of whether it has unpushed work.
pub(crate) fn scan_repo_tops(root: &Path, include_worktrees: bool) -> anyhow::Result<Vec<RepoTop>> {
    discover_git_repos(root, include_worktrees)?
        .iter()
        .map(|repo| {
            let top = git::top_level(repo)?;
            let label = repo_label(root, Path::new(&top));
            Ok(RepoTop {
                top: PathBuf::from(top),
                label,
            })
        })
        .collect()
}

/// Load managed repos from `options` and keep only those with unpushed commits against
/// their upstream: skips repos without a `.git` dir, without a resolvable upstream, or
/// with nothing unpushed (`unpushed_count` == 0).
pub(crate) fn unpushed_managed_repo_tops(options: &ManagedOptions) -> anyhow::Result<Vec<RepoTop>> {
    let repos = managed::load_repos(options)?;
    let mut tops = Vec::new();
    for repo in repos.iter().filter(|repo| repo.path.join(".git").exists()) {
        if git::upstream(&repo.path).is_err() || unpushed_count(&repo.path)? == 0 {
            continue;
        }
        let top = git::top_level(&repo.path)?;
        tops.push(RepoTop {
            top: PathBuf::from(top),
            label: repo.name.clone(),
        });
    }
    Ok(tops)
}

/// Discover repos under `root` and render them through `backend`, printing its
/// wire notes and opening the artifact. Split from [`run_scan`] so tests can
/// drive a fake backend. Repo discovery stays cli-side (filesystem walking).
pub(crate) fn run_scan_with(
    backend: &impl Backend,
    root: impl AsRef<Path>,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
) -> anyhow::Result<DiffOutcome> {
    let root = std::fs::canonicalize(root.as_ref())
        .with_context(|| format!("failed to resolve {}", root.as_ref().display()))?;
    let repo_tops = scan_repo_tops(&root, include_worktrees)?;
    if repo_tops.is_empty() {
        anyhow::bail!("diff -r: no git repos found under {}", root.display());
    }
    let repo_refs = repo_tops
        .into_iter()
        .map(|repo_top| RepoRefDto {
            top: repo_top.top.to_string_lossy().into_owned(),
            label: repo_top.label,
        })
        .collect();

    let req = RenderDiffSubreposRequest {
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
        root: root.to_string_lossy().into_owned(),
        target: super::to_target_dto(&last.map_or(DiffTarget::Unpushed, DiffTarget::Last)),
        repos: repo_refs,
        theme: crate::config::load().theme,
    };
    let envelope = backend.render_diff_subrepos(&req)?;
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

/// `diff --all`: default forwards one recipe batch (one recipe per managed repo with
/// unpushed commits) to the viewer (no daemon, no artifact); `--raw`, or no display,
/// keeps the daemon/store/browser path ([`run_managed_all_with`]).
pub fn run_managed_all(
    root: impl AsRef<Path>,
    options: &ManagedOptions,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    if take_raw_path(raw, viewer::has_display()) {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return run_managed_all_with(&backend, root, options).map(DiffOutcome::Rendered);
    }
    let root = root.as_ref();
    forward_managed_all(root, options, super::forward_recipes, || {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        run_managed_all_with(&backend, root, options).map(DiffOutcome::Rendered)
    })
}

/// Resolve `root`'s managed, unpushed-only repos into a recipe batch and hand it to
/// `forward`. On a forward failure (no viewer binary, or spawn failed) degrade to
/// `degrade` (the raw path) rather than failing — FSD A-0002. A genuine resolution error
/// still propagates. An empty recipe list short-circuits before forwarding: with a display
/// present there is nothing to render, so spawning the viewer would be a silent no-op.
/// Split from [`run_managed_all`] so tests can drive a fake forwarder + backend instead of
/// spawning the real viewer.
pub(crate) fn forward_managed_all(
    root: &Path,
    options: &ManagedOptions,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
    degrade: impl FnOnce() -> anyhow::Result<DiffOutcome>,
) -> anyhow::Result<DiffOutcome> {
    let recipes = recipe::managed_recipes(root, options)?;
    if recipes.is_empty() {
        println!("diff --all: no managed repos with unpushed commits");
        return Ok(DiffOutcome::Empty);
    }
    let batch = OpenRecipes {
        batch_id: recipe::new_batch_id(),
        recipes,
    };
    match forward(&batch) {
        Ok(()) => Ok(DiffOutcome::Forwarded),
        Err(err) => {
            super::note_viewer_degrade(&err);
            degrade()
        }
    }
}

/// Pre-filter managed repos with unpushed commits and render them through
/// `backend`, printing its wire notes and opening the artifact. Split from
/// [`run_managed_all`] so tests can drive a fake backend. The pre-filter loop
/// stays cli-side (uses the `git` shim, not a port).
pub(crate) fn run_managed_all_with(
    backend: &impl Backend,
    root: impl AsRef<Path>,
    options: &ManagedOptions,
) -> anyhow::Result<PathBuf> {
    let root = std::fs::canonicalize(root.as_ref())
        .with_context(|| format!("failed to resolve {}", root.as_ref().display()))?;
    let repo_refs = unpushed_managed_repo_tops(options)?
        .into_iter()
        .map(|repo_top| RepoRefDto {
            top: repo_top.top.to_string_lossy().into_owned(),
            label: repo_top.label,
        })
        .collect();

    let req = RenderDiffAllRequest {
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
        root: root.to_string_lossy().into_owned(),
        repos: repo_refs,
        theme: crate::config::load().theme,
    };
    let envelope = backend.render_diff_all(&req)?;
    super::print_wire_notes(&envelope.notes);
    match envelope.outcome {
        Outcome::Ok => {
            let data = envelope.data.context("daemon returned ok without data")?;
            let artifact = PathBuf::from(data.artifact);
            super::open_artifact(&artifact);
            Ok(artifact)
        }
        _ => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
    }
}

fn unpushed_count(repo: &Path) -> anyhow::Result<usize> {
    let raw = git::run_git(repo, &["rev-list", "--count", "@{u}..HEAD"])?;
    Ok(raw.trim().parse().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use contracts::{diffs::RenderDiffData, envelope::Envelope};

    use super::*;

    /// Records whether `render_diff_subrepos` was invoked, and returns an `Empty` outcome
    /// so the raw path completes without opening anything — lets the degrade test assert
    /// the raw backend was reached without spawning a viewer or writing an artifact.
    struct TrackingBackend<'a>(&'a Cell<bool>);

    impl Backend for TrackingBackend<'_> {
        fn render_diff_subrepos(
            &self,
            _req: &RenderDiffSubreposRequest,
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
    fn forward_scan_forwards_one_recipe_per_discovered_repo_and_calls_no_backend() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("api")).unwrap();
        init_repo(&root.join("api"));
        std::fs::create_dir_all(root.join("web")).unwrap();
        init_repo(&root.join("web"));
        let captured: RefCell<Option<OpenRecipes>> = RefCell::new(None);

        let outcome = forward_scan(
            root,
            None,
            false,
            |batch| {
                *captured.borrow_mut() = Some(batch.clone());
                Ok(())
            },
            || unreachable!("degrade must not run when forwarding succeeds"),
        )
        .unwrap();

        assert!(matches!(outcome, DiffOutcome::Forwarded));
        let batch = captured.into_inner().expect("forward must be called");
        assert_eq!(batch.recipes.len(), 2);
    }

    #[test]
    fn forward_scan_degrades_to_the_raw_backend_when_forwarding_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        std::fs::create_dir_all(root.join("api")).unwrap();
        init_repo(&root.join("api"));
        let backend_called = Cell::new(false);
        let backend = TrackingBackend(&backend_called);

        let outcome = forward_scan(
            &root,
            None,
            false,
            |_batch| anyhow::bail!("gtl-viewer is not installed"),
            || run_scan_with(&backend, &root, None, false),
        )
        .unwrap();

        assert!(
            backend_called.get(),
            "a forward failure must degrade to the raw backend, not error out"
        );
        assert!(matches!(outcome, DiffOutcome::Empty));
    }

    #[test]
    fn forward_scan_errors_when_no_repos_are_found() {
        let tmp = tempfile::tempdir().unwrap();

        let result = forward_scan(
            tmp.path(),
            None,
            false,
            |_batch| Ok(()),
            || unreachable!("a genuine 'no repos found' error must propagate, not degrade"),
        );

        assert!(result.is_err());
    }

    /// Set up one managed repo with an unpushed commit under `tmp`, returning the
    /// `ManagedOptions` pointing at it. Shared by the tests that need `forward_managed_all`
    /// to actually resolve a non-empty recipe batch.
    fn managed_options_with_one_unpushed_repo(tmp: &std::path::Path) -> ManagedOptions {
        let home = tmp.join("home");
        std::fs::create_dir_all(home.join("repo1")).unwrap();
        init_repo(&home.join("repo1"));
        let remote = tmp.join("origin.git");
        assert!(
            std::process::Command::new("git")
                .args(["init", "--bare"])
                .arg(&remote)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(home.join("repo1"))
                .args(["remote", "add", "origin", remote.to_str().unwrap()])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(home.join("repo1"))
                .args(["push", "-u", "origin", "HEAD"])
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(home.join("repo1").join("a.txt"), "a\nmore\n").unwrap();
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(home.join("repo1"))
                .args(["commit", "-aqm", "second"])
                .status()
                .unwrap()
                .success()
        );

        let manifest = tmp.join("repos.toml");
        std::fs::write(
            &manifest,
            "[[repo]]\npath = \"repo1\"\nremote = \"origin\"\n",
        )
        .unwrap();
        ManagedOptions {
            repos_file: Some(manifest),
            home_dir: Some(home),
            dry: false,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        }
    }

    #[test]
    fn forward_managed_all_forwards_one_recipe_per_unpushed_repo_and_calls_no_backend() {
        let tmp = tempfile::tempdir().unwrap();
        let options = managed_options_with_one_unpushed_repo(tmp.path());
        let captured: RefCell<Option<OpenRecipes>> = RefCell::new(None);

        let outcome = forward_managed_all(
            tmp.path(),
            &options,
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
    }

    #[test]
    fn forward_managed_all_degrades_to_the_raw_backend_when_forwarding_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let options = managed_options_with_one_unpushed_repo(tmp.path());
        let degraded = Cell::new(false);

        let outcome = forward_managed_all(
            tmp.path(),
            &options,
            |_batch| anyhow::bail!("gtl-viewer is not installed"),
            || {
                degraded.set(true);
                Ok(DiffOutcome::Empty)
            },
        )
        .unwrap();

        assert!(
            degraded.get(),
            "a forward failure must degrade to the raw path, not error out"
        );
        assert!(matches!(outcome, DiffOutcome::Empty));
    }

    #[test]
    fn forward_managed_all_short_circuits_on_an_empty_recipe_batch_without_forwarding() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("repos.toml");
        std::fs::write(&manifest, "").unwrap(); // no managed repos -> empty batch
        let options = ManagedOptions {
            repos_file: Some(manifest),
            home_dir: Some(tmp.path().to_path_buf()),
            dry: false,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        };

        let outcome = forward_managed_all(
            tmp.path(),
            &options,
            |_batch| unreachable!("an empty recipe batch must never be forwarded"),
            || unreachable!("an empty recipe batch must not degrade to the raw backend either"),
        )
        .unwrap();

        assert!(matches!(outcome, DiffOutcome::Empty));
    }
}
