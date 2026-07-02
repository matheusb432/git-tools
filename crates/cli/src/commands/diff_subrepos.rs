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
        diff::DiffOutcome,
        discover::{discover_git_repos, repo_label},
        managed::{self, ManagedOptions},
    },
    git,
};

pub fn run_scan(
    root: impl AsRef<Path>,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
) -> anyhow::Result<DiffOutcome> {
    let backend = crate::client::HttpBackend::ensure_daemon()?;
    run_scan_with(&backend, root, last, include_worktrees)
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
    let repos = discover_git_repos(&root, include_worktrees)?;
    if repos.is_empty() {
        anyhow::bail!("diff subrepos: no git repos found under {}", root.display());
    }
    let repo_refs = repos
        .iter()
        .map(|repo| {
            let top = git::top_level(repo)?;
            let label = repo_label(&root, Path::new(&top));
            Ok(RepoRefDto { top, label })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

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
            Ok(DiffOutcome::Rendered(PathBuf::from(data.artifact)))
        }
        Outcome::Empty => Ok(DiffOutcome::Empty),
        Outcome::Error => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
    }
}

pub fn run_managed_all(
    root: impl AsRef<Path>,
    options: &ManagedOptions,
) -> anyhow::Result<PathBuf> {
    let backend = crate::client::HttpBackend::ensure_daemon()?;
    run_managed_all_with(&backend, root, options)
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
    let repos = managed::load_repos(options)?;

    let mut repo_refs = Vec::new();
    for repo in repos.iter().filter(|repo| repo.path.join(".git").exists()) {
        if git::upstream(&repo.path).is_err() || unpushed_count(&repo.path)? == 0 {
            continue;
        }
        let top = git::top_level(&repo.path)?;
        repo_refs.push(RepoRefDto {
            top,
            label: repo.name.clone(),
        });
    }

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
            Ok(PathBuf::from(data.artifact))
        }
        _ => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
    }
}

fn unpushed_count(repo: &Path) -> anyhow::Result<usize> {
    let raw = git::run_git(repo, &["rev-list", "--count", "@{u}..HEAD"])?;
    Ok(raw.trim().parse().unwrap_or(0))
}
