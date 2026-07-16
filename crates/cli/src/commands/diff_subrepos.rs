use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use anyhow::Context;
use contracts::{
    diffs::{RenderDiffAllRequest, RenderDiffSubreposRequest, RepoRefDto},
    envelope::Outcome,
};
use gtl_recipe::{OpenRecipes, RecipeBatchKind};

use crate::{
    cli::DiffTarget,
    client::Backend,
    commands::{
        diff::{DiffOutcome, take_raw_path},
        managed::ManagedOptions,
    },
    recipe, viewer,
};

/// `diff -r`: raw/headless invocations render one tabbed store artifact; displayed
/// app-default invocations forward every discovered repo in one recipe batch.
pub fn run_scan(
    root: impl AsRef<Path>,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    if take_raw_path(raw, viewer::has_display()) {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return run_scan_with(
            &backend,
            root,
            last,
            include_worktrees,
            super::open_artifact,
        );
    }
    if viewer::no_open_requested() {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return run_scan_with(&backend, root, last, include_worktrees, super::do_not_open);
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
            run_scan_with(
                &backend,
                &root,
                last,
                include_worktrees,
                super::open_artifact,
            )
        },
    )
}

pub(crate) fn forward_scan(
    root: &Path,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
    degrade: impl FnOnce() -> anyhow::Result<DiffOutcome>,
) -> anyhow::Result<DiffOutcome> {
    let target = last.map_or(DiffTarget::Unpushed { pinned: None }, |count| {
        DiffTarget::Last {
            count,
            pinned: None,
        }
    });
    let recipes = recipe::subrepo_recipes(root, target, include_worktrees)?;
    if recipes.is_empty() {
        anyhow::bail!("diff -r: no git repos found under {}", root.display());
    }
    forward_batch(recipes, forward, degrade)
}

/// Discover git repos under `root` and resolve each to its canonical top-level path +
/// a label relative to `root` — one call into the `discovery/find_repo_tops` slice
/// with the production adapters. No unpushed filtering — `diff -r` renders every
/// discovered repo regardless of whether it has unpushed work.
fn scan_repo_tops(
    root: &Path,
    include_worktrees: bool,
) -> anyhow::Result<Vec<domain::discovery::DiscoveredRepo>> {
    Ok(application::discovery::find_repo_tops::execute(
        application::discovery::find_repo_tops::FindRepoTops {
            root: root.to_path_buf(),
            include_worktrees,
        },
        &infra::repo_discovery::WalkdirRepoDiscovery,
        &infra::git_runner::StdGitRunner,
    )?)
}

/// Discover repos under `root` and render them through `backend`, printing its
/// wire notes and opening the artifact. Split from [`run_scan`] so tests can
/// drive a fake backend. Repo discovery stays cli-side (filesystem walking).
pub(crate) fn run_scan_with(
    backend: &impl Backend,
    root: impl AsRef<Path>,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
    open: impl FnOnce(&Path),
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
            top: repo_top.path.to_string_lossy().into_owned(),
            label: repo_top.label,
        })
        .collect();

    let req = RenderDiffSubreposRequest {
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
        root: root.to_string_lossy().into_owned(),
        target: super::to_target_dto(&last.map_or(
            DiffTarget::Unpushed { pinned: None },
            |count| DiffTarget::Last {
                count,
                pinned: None,
            },
        )),
        repos: repo_refs,
        theme: infra::user_config::load().theme,
    };
    let envelope = backend.render_diff_subrepos(&req)?;
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

/// `diff --all`: raw/headless invocations render one tabbed store artifact; displayed
/// app-default invocations forward every managed repo with unpushed commits in one batch.
pub fn run_managed_all(
    root: impl AsRef<Path>,
    options: &ManagedOptions,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    if take_raw_path(raw, viewer::has_display()) {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return run_managed_all_with(&backend, root, options, super::open_artifact)
            .map(DiffOutcome::Rendered);
    }
    if viewer::no_open_requested() {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        return run_managed_all_with(&backend, root, options, super::do_not_open)
            .map(DiffOutcome::Rendered);
    }
    let root = root.as_ref();
    forward_managed_all(root, options, super::forward_recipes, || {
        let backend = crate::client::HttpBackend::ensure_daemon()?;
        run_managed_all_with(&backend, root, options, super::open_artifact)
            .map(DiffOutcome::Rendered)
    })
}

pub(crate) fn forward_managed_all(
    root: &Path,
    options: &ManagedOptions,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
    degrade: impl FnOnce() -> anyhow::Result<DiffOutcome>,
) -> anyhow::Result<DiffOutcome> {
    std::fs::canonicalize(root)
        .map_err(|error| anyhow::anyhow!("failed to resolve {}: {error}", root.display()))?;
    let recipes = recipe::managed_recipes(
        options,
        application::recipes::RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
    )?;
    if recipes.is_empty() {
        println!("diff --all: no managed repos with unpushed commits");
        return Ok(DiffOutcome::Empty);
    }
    forward_batch(recipes, forward, degrade)
}

fn forward_batch(
    recipes: Vec<gtl_recipe::Recipe>,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
    degrade: impl FnOnce() -> anyhow::Result<DiffOutcome>,
) -> anyhow::Result<DiffOutcome> {
    let batch = OpenRecipes {
        batch_id: recipe::new_batch_id(),
        kind: RecipeBatchKind::Snapshot,
        recipes,
    };
    match forward(&batch) {
        Ok(()) => Ok(DiffOutcome::Forwarded),
        Err(error) => {
            super::note_viewer_degrade(&error);
            degrade()
        }
    }
}

/// Pre-filter managed repos with unpushed commits and render them through `backend`,
/// printing its wire notes and handing the artifact to `open`. Split from
/// [`run_managed_all`] so tests can drive a fake backend.
pub(crate) fn run_managed_all_with(
    backend: &impl Backend,
    root: impl AsRef<Path>,
    options: &ManagedOptions,
    open: impl FnOnce(&Path),
) -> anyhow::Result<PathBuf> {
    let root = std::fs::canonicalize(root.as_ref())
        .with_context(|| format!("failed to resolve {}", root.as_ref().display()))?;
    let repo_refs = recipe::selected_managed_repos(options)?
        .into_iter()
        .map(|repo_top| RepoRefDto {
            top: repo_top.path.to_string_lossy().into_owned(),
            label: repo_top.label,
        })
        .collect();

    let req = RenderDiffAllRequest {
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
        root: root.to_string_lossy().into_owned(),
        repos: repo_refs,
        theme: infra::user_config::load().theme,
    };
    let envelope = backend.render_diff_all(&req)?;
    super::print_wire_notes(&envelope.notes);
    match envelope.outcome {
        Outcome::Ok => {
            let data = envelope.data.context("daemon returned ok without data")?;
            let artifact = PathBuf::from(data.artifact);
            open(&artifact);
            Ok(artifact)
        }
        _ => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn init_repo(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
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

    #[test]
    fn recursive_app_path_forwards_every_discovered_repo_in_one_batch() {
        let root = tempfile::tempdir().unwrap();
        init_repo(&root.path().join("api"));
        init_repo(&root.path().join("web"));
        let captured = RefCell::new(None);

        let outcome = forward_scan(
            root.path(),
            None,
            false,
            |batch| {
                *captured.borrow_mut() = Some(batch.clone());
                Ok(())
            },
            || unreachable!("successful forwarding must not degrade"),
        )
        .unwrap();

        assert!(matches!(outcome, DiffOutcome::Forwarded));
        let batch = captured.into_inner().unwrap();
        assert_eq!(batch.recipes.len(), 2);
        let mut names = batch
            .recipes
            .into_iter()
            .map(|recipe| recipe.name.unwrap())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["api", "web"]);
    }
}
