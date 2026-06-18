use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::cli::DiffTarget;
use crate::commands::managed::{self, ManagedOptions};
use crate::git;
use crate::open::open_file;
use crate::render::build_tabbed_html;

pub fn run_scan(root: impl AsRef<Path>, last: Option<NonZeroU32>) -> anyhow::Result<PathBuf> {
    let root = std::fs::canonicalize(root.as_ref())
        .with_context(|| format!("failed to resolve {}", root.as_ref().display()))?;
    let repos = discover_git_repos(&root)?;
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
    let out_file = super::output_file(&root, "diff-preview-subrepos.html", &html)?;

    println!("diff subrepos: {} repo(s)", views.len());
    println!("wrote {}", out_file.display());
    open_file(&out_file);
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
    let out_file = super::output_file(&root, "diff-preview-all.html", &html)?;

    println!("diff-all: {} repo(s)", views.len());
    println!("wrote {}", out_file.display());
    open_file(&out_file);
    Ok(out_file)
}

fn unpushed_count(repo: &Path) -> anyhow::Result<usize> {
    let raw = git::run_git(repo, &["rev-list", "--count", "@{u}..HEAD"])?;
    Ok(raw.trim().parse().unwrap_or(0))
}

fn discover_git_repos(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut repos = Vec::new();
    collect_git_repos(root, &mut repos)?;
    repos.sort();
    Ok(repos)
}

fn collect_git_repos(dir: &Path, repos: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if dir.file_name().is_some_and(|name| name == ".git") {
        return Ok(());
    }
    if dir.join(".git").exists() {
        repos.push(dir.to_path_buf());
    }

    let mut entries = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read directory entry under {}", dir.display()))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_git_repos(&path, repos)?;
        }
    }

    Ok(())
}

fn repo_label(root: &Path, repo: &Path) -> String {
    let relative = repo.strip_prefix(root).unwrap_or(repo);
    let label = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if label.is_empty() {
        super::repo_name(repo)
    } else {
        label
    }
}
