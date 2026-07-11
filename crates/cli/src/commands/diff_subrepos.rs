use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use anyhow::Context;
use contracts::{
    diffs::{RenderDiffAllRequest, RenderDiffSubreposRequest, RepoRefDto},
    envelope::Outcome,
};

use crate::{
    cli::DiffTarget,
    client::Backend,
    commands::{
        diff::{DiffOutcome, take_raw_path},
        discover::{discover_git_repos, repo_label},
        managed::{self, ManagedOptions},
    },
    git, viewer,
};

/// `diff -r`: render every discovered repo through the daemon and open the resulting
/// tabbed store artifact — the `--raw`/headless path opens it in the browser, the default
/// path hands it to the desktop viewer as a `diff://` url ([`run_scan_with`]).
pub fn run_scan(
    root: impl AsRef<Path>,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    let open: fn(&Path) = if take_raw_path(raw, viewer::has_display()) {
        super::open_artifact
    } else {
        super::open_in_viewer
    };
    let backend = crate::client::HttpBackend::ensure_daemon()?;
    run_scan_with(&backend, root, last, include_worktrees, open)
}

/// A discovered or pre-filtered repo, resolved to its canonical top-level path and a
/// display label. The unit shared by the raw `RepoRefDto` wire paths
/// ([`run_scan_with`], [`run_managed_all_with`]), so discovery and pre-filtering are
/// implemented exactly once.
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
            open(&artifact);
            Ok(DiffOutcome::Rendered(artifact))
        }
        Outcome::Empty => Ok(DiffOutcome::Empty),
        Outcome::Error => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
    }
}

/// `diff --all`: render every managed repo with unpushed commits through the daemon's
/// `/diffs/all` endpoint and open the resulting single tabbed store artifact — the
/// `--raw`/headless path opens it in the browser, the default path hands it to the
/// desktop viewer as a `diff://` url ([`run_managed_all_with`]).
pub fn run_managed_all(
    root: impl AsRef<Path>,
    options: &ManagedOptions,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    let open: fn(&Path) = if take_raw_path(raw, viewer::has_display()) {
        super::open_artifact
    } else {
        super::open_in_viewer
    };
    let backend = crate::client::HttpBackend::ensure_daemon()?;
    run_managed_all_with(&backend, root, options, open).map(DiffOutcome::Rendered)
}

/// Pre-filter managed repos with unpushed commits and render them through `backend`,
/// printing its wire notes and handing the artifact to `open`. Split from
/// [`run_managed_all`] so tests can drive a fake backend. The pre-filter loop
/// stays cli-side (uses the `git` shim, not a port).
pub(crate) fn run_managed_all_with(
    backend: &impl Backend,
    root: impl AsRef<Path>,
    options: &ManagedOptions,
    open: impl FnOnce(&Path),
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
            open(&artifact);
            Ok(artifact)
        }
        _ => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
    }
}

fn unpushed_count(repo: &Path) -> anyhow::Result<usize> {
    let raw = git::run_git(repo, &["rev-list", "--count", "@{u}..HEAD"])?;
    Ok(raw.trim().parse().unwrap_or(0))
}
