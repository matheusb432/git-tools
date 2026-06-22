use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::cli::DiffTarget;
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

fn discover_git_repos(root: &Path, include_worktrees: bool) -> anyhow::Result<Vec<PathBuf>> {
    let mut repos = Vec::new();
    collect_git_repos(root, include_worktrees, &mut repos)?;
    repos.sort();
    Ok(repos)
}

fn collect_git_repos(
    dir: &Path,
    include_worktrees: bool,
    repos: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    if dir.file_name().is_some_and(|name| name == ".git") {
        return Ok(());
    }
    // ! A linked worktree carries a full working tree of its own; skipping it (and its
    // ! subtree) by default keeps the scan to real, independent repos.
    if !include_worktrees && is_linked_worktree(dir) {
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
            collect_git_repos(&path, include_worktrees, repos)?;
        }
    }

    Ok(())
}

// ! A linked worktree's `.git` is a file (not a dir) whose `gitdir:` points into another
// ! repo's `.git/worktrees/<name>` — that pointer is git's own marker, robust to layout.
fn is_linked_worktree(dir: &Path) -> bool {
    let git_path = dir.join(".git");
    if !git_path.is_file() {
        return false;
    }
    let Ok(content) = std::fs::read_to_string(&git_path) else {
        return false;
    };
    content
        .lines()
        .filter_map(|line| line.strip_prefix("gitdir:"))
        .any(|target| target.trim().replace('\\', "/").contains("/worktrees/"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_repo(dir: &Path) {
        fs::create_dir_all(dir.join(".git")).unwrap();
    }

    fn make_worktree(dir: &Path, gitdir: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(".git"), format!("gitdir: {gitdir}\n")).unwrap();
    }

    #[test]
    fn is_linked_worktree_detects_worktree_pointer() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        make_worktree(&wt, "/repo/.git/worktrees/feature");
        assert!(is_linked_worktree(&wt));
    }

    #[test]
    fn is_linked_worktree_ignores_plain_repo_and_submodule() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        make_repo(&repo);
        assert!(!is_linked_worktree(&repo));

        let submodule = tmp.path().join("sub");
        make_worktree(&submodule, "/repo/.git/modules/sub");
        assert!(!is_linked_worktree(&submodule));
    }

    #[test]
    fn discover_skips_nested_worktrees_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_repo(&root.join("api"));
        make_worktree(
            &root.join("api/.worktrees/feature"),
            "/abs/api/.git/worktrees/feature",
        );

        let repos = discover_git_repos(root, false).unwrap();
        assert_eq!(repos, vec![root.join("api")]);
    }

    #[test]
    fn discover_includes_worktrees_with_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_repo(&root.join("api"));
        let wt = root.join("api/.worktrees/feature");
        make_worktree(&wt, "/abs/api/.git/worktrees/feature");

        let repos = discover_git_repos(root, true).unwrap();
        assert!(repos.contains(&root.join("api")));
        assert!(repos.contains(&wt));
    }
}
