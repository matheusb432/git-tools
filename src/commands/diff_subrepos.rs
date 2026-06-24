use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::cli::DiffTarget;
use crate::commands::discover::{discover_git_repos, repo_label};
use crate::commands::managed::{self, ManagedOptions};
use crate::git;
use crate::render::build_tabbed_html;

pub fn run_scan(
    root: impl AsRef<Path>,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
) -> anyhow::Result<PathBuf> {
    let root = std::fs::canonicalize(root.as_ref())
        .with_context(|| format!("failed to resolve {}", root.as_ref().display()))?;
    let repos = discover_git_repos(&root, include_worktrees)?;
    if repos.is_empty() {
        anyhow::bail!("diff subrepos: no git repos found under {}", root.display());
    }

    let target = last.map_or(DiffTarget::Unpushed, DiffTarget::Last);
    let mut views = Vec::with_capacity(repos.len());
    for repo in &repos {
        let top = git::top_level(repo)?;
        let (mut view, _) = super::diff::build_view(&top, &target)?;
        view.repo_name = repo_label(&root, Path::new(&top));
        views.push(view);
    }

    let html = build_tabbed_html("diff-preview subrepos", &views);
    let meta = super::ArtifactMeta {
        repo_root: root.to_string_lossy().to_string(),
        repo_name: "subrepos".to_string(),
        kind: gtl_store::DiffKind::WorkTree,
        base_sha: String::new(),
        head_sha: String::new(),
        range_label: String::new(),
        head_committed_at: String::new(),
        title: "diff-preview subrepos".to_string(),
    };
    let out_file = super::store_artifact(&meta, &html)?;

    println!("diff subrepos: {} repo(s)", views.len());
    println!("wrote {}", out_file.display());
    super::open_artifact(&out_file);
    Ok(out_file)
}

pub fn run_managed_all(
    root: impl AsRef<Path>,
    options: &ManagedOptions,
) -> anyhow::Result<PathBuf> {
    let root = std::fs::canonicalize(root.as_ref())
        .with_context(|| format!("failed to resolve {}", root.as_ref().display()))?;
    let repos = managed::load_repos(options)?;

    let mut views = Vec::new();
    for repo in repos.iter().filter(|repo| repo.path.join(".git").exists()) {
        if git::upstream(&repo.path).is_err() || unpushed_count(&repo.path)? == 0 {
            continue;
        }
        let top = git::top_level(&repo.path)?;
        let (mut view, _) = super::diff::build_view(&top, &DiffTarget::Unpushed)?;
        view.repo_name = repo.name.clone();
        views.push(view);
    }

    let html = build_tabbed_html("diff-preview all", &views);
    let meta = super::ArtifactMeta {
        repo_root: root.to_string_lossy().to_string(),
        repo_name: "all".to_string(),
        kind: gtl_store::DiffKind::WorkTree,
        base_sha: String::new(),
        head_sha: String::new(),
        range_label: String::new(),
        head_committed_at: String::new(),
        title: "diff-preview all".to_string(),
    };
    let out_file = super::store_artifact(&meta, &html)?;

    println!("diff-all: {} repo(s)", views.len());
    println!("wrote {}", out_file.display());
    super::open_artifact(&out_file);
    Ok(out_file)
}

fn unpushed_count(repo: &Path) -> anyhow::Result<usize> {
    let raw = git::run_git(repo, &["rev-list", "--count", "@{u}..HEAD"])?;
    Ok(raw.trim().parse().unwrap_or(0))
}
