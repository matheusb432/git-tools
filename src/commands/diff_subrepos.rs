use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use anyhow::Context;

use crate::{
    cli::DiffTarget,
    commands::{
        diff::DiffOutcome,
        discover::{discover_git_repos, repo_label},
        managed::{self, ManagedOptions},
    },
    git,
    render::build_tabbed_html,
};

pub fn run_scan(
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

    // Build a view per repo, but keep only the ones with something to show — an empty
    // range (no commits or changes) would render a blank tab that reads as a bug, exactly
    // the case single-repo `diff` already skips. Skipped repos are reported to the terminal
    // so the empty result is visible; only relevant repos reach the template.
    let target = last.map_or(DiffTarget::Unpushed, DiffTarget::Last);
    let mut views = Vec::with_capacity(repos.len());
    let mut skipped = 0usize;
    for repo in &repos {
        let top = git::top_level(repo)?;
        let label = repo_label(&root, Path::new(&top));
        match super::diff::build_view(&top, &target) {
            Ok((mut view, _)) if !view.is_empty() => {
                view.repo_name = label;
                views.push(view);
            }
            // Empty range (nothing to preview), or a repo whose view can't be built — either
            // way it has nothing to show. Skipped repos are summarized below, not listed
            // one-by-one, so a tree full of up-to-date repos stays quiet.
            Ok(_) | Err(_) => skipped += 1,
        }
    }

    if views.is_empty() {
        eprintln!(
            "diff subrepos: nothing to show across {} repo(s); no preview written",
            repos.len()
        );
        return Ok(DiffOutcome::Empty);
    }

    let title = dated_title("diff-preview subrepos");
    let html = build_tabbed_html(&title, &views);
    let meta = super::ArtifactMeta {
        repo_root: root.to_string_lossy().to_string(),
        repo_name: "subrepos".to_string(),
        kind: gtl_store::DiffKind::WorkTree,
        base_sha: String::new(),
        head_sha: String::new(),
        range_label: String::new(),
        head_committed_at: String::new(),
        title,
    };
    let out_file = super::store_artifact(&meta, &html)?;

    println!("diff subrepos: {} repo(s)", views.len());
    if skipped > 0 {
        eprintln!("diff subrepos: skipped {skipped} repo(s) with nothing to show");
    }
    println!("wrote {}", out_file.display());
    super::open_artifact(&out_file);
    Ok(DiffOutcome::Rendered(out_file))
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

    let title = dated_title("diff-preview all");
    let html = build_tabbed_html(&title, &views);
    let meta = super::ArtifactMeta {
        repo_root: root.to_string_lossy().to_string(),
        repo_name: "all".to_string(),
        kind: gtl_store::DiffKind::WorkTree,
        base_sha: String::new(),
        head_sha: String::new(),
        range_label: String::new(),
        head_committed_at: String::new(),
        title,
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

/// Prefixes a static template label with today's UTC date so repeated runs of
/// `diff subrepos` / `diff-all` produce distinguishable history-panel titles
/// instead of all sharing one generic label.
fn dated_title(label: &str) -> String {
    let date = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date();
    format!("{date} {label}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dated_title_prefixes_label_with_yyyy_mm_dd() {
        let title = dated_title("diff-preview subrepos");

        let (date, label) = title.split_once(' ').expect("title has a date prefix");
        assert_eq!(date.len(), 10);
        assert!(date.chars().enumerate().all(|(i, c)| match i {
            4 | 7 => c == '-',
            _ => c.is_ascii_digit(),
        }));
        assert_eq!(label, "diff-preview subrepos");
    }
}
