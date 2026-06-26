use std::path::{Path, PathBuf};

use crate::{
    cli::DiffTarget,
    commands::{Mode, legacy_count_label, legacy_unpushed_commit_label, ranges, repo_name},
    git,
    model::View,
    render::build_html,
};

/// Outcome of a single `diff` invocation: either an artifact was written, or the
/// range was empty and we deliberately skipped rendering a blank preview.
pub enum DiffOutcome {
    Rendered(PathBuf),
    Empty,
}

pub fn run(target: &DiffTarget, name: Option<&str>) -> anyhow::Result<DiffOutcome> {
    let top = git::top_level(".")?;
    render(&top, target, name)
}

// ! base = text before `..`/`...`; for worktree mode (no `..`) the whole string is the base.
fn range_base(range: &str) -> &str {
    range
        .split("..")
        .next()
        .unwrap_or(range)
        .trim_end_matches('.')
}

// ! head = text after the last `..`; worktree mode has no commit head ⇒ sentinel.
fn head_sha_for(top: &str, range: &str) -> String {
    if range.contains("..") {
        let tip = range.rsplit("..").next().unwrap_or("HEAD");
        crate::git::resolve_sha(top, tip).unwrap_or_default()
    } else {
        "WORKTREE".to_string()
    }
}

pub(crate) fn render(
    top: &str,
    target: &DiffTarget,
    name: Option<&str>,
) -> anyhow::Result<DiffOutcome> {
    // ! Fast-path: pure commit ranges are fully determined by resolved shas, so a
    // ! prior identical artifact can be reused without the expensive assemble.
    if name.is_none()
        && let Some(hit) = range_fast_path(top, target)?
    {
        println!("diff-preview: reusing {}", hit.display());
        super::open_artifact(&hit);
        return Ok(DiffOutcome::Rendered(hit));
    }
    let (mut view, summary) = build_view(top, target)?;
    if let Some(name) = name {
        view.title = name.to_string();
    }
    if view.is_empty() {
        eprintln!("diff-preview: {summary} — nothing to show (no commits or changes); skipping");
        return Ok(DiffOutcome::Empty);
    }
    let file_count = view.files.len();
    let html = build_html(&view);

    let meta = super::ArtifactMeta {
        repo_root: top.to_string(),
        repo_name: view.repo_name.clone(),
        kind: gtl_store::DiffKind::from_diff_range(&view.cmd.range),
        base_sha: crate::git::resolve_sha(top, range_base(&view.cmd.range)).unwrap_or_default(),
        head_sha: head_sha_for(top, &view.cmd.range),
        range_label: view.cmd.range.clone(),
        head_committed_at: crate::git::committed_at(top, "HEAD"),
        title: view.title.clone(),
    };
    let out_file = super::store_artifact(&meta, &html)?;

    println!(
        "diff-preview: {summary}, {}",
        legacy_count_label(file_count, "file")
    );
    println!("wrote {}", out_file.display());
    super::open_artifact(&out_file);
    Ok(DiffOutcome::Rendered(out_file))
}

pub(crate) fn build_view(top: &str, target: &DiffTarget) -> anyhow::Result<(View, String)> {
    let branch = git::current_branch(top)?;
    let repo_name = repo_name(top);

    let (base_ref, io_ranges, view_ranges, fallback_to_main) = match target {
        DiffTarget::Range(range) => {
            verify_exact_range(top, range)?;
            (
                range.clone(),
                ranges(range, Mode::ExactRange),
                ranges(range, Mode::ExactRange),
                false,
            )
        }
        DiffTarget::Base(base) => {
            git::verify_commit(top, base)?;
            let short = git::short_ref(top, base)?;
            (
                short.clone(),
                ranges(base, Mode::Hash),
                ranges(&short, Mode::Hash),
                false,
            )
        }
        DiffTarget::Merge(base) => {
            git::verify_commit(top, base)?;
            (
                base.clone(),
                ranges(base, Mode::Merge),
                ranges(base, Mode::Merge),
                false,
            )
        }
        DiffTarget::Unpushed => {
            let base = unpushed_or_main_base(top)?;
            let mode = if base.is_upstream {
                Mode::Unpushed
            } else {
                Mode::Hash
            };
            (
                base.ref_name.clone(),
                ranges(&base.ref_name, mode),
                ranges(&base.ref_name, mode),
                !base.is_upstream,
            )
        }
        DiffTarget::Last(count) => {
            // * "last N" is just the HEAD~N..HEAD range; reuse the verified ExactRange plumbing.
            let range = format!("HEAD~{count}..HEAD");
            verify_exact_range(top, &range)?;
            (
                range.clone(),
                ranges(&range, Mode::ExactRange),
                ranges(&range, Mode::ExactRange),
                false,
            )
        }
    };

    let crate::diff::DiffData { commits, mut files } = crate::diff::assemble(
        top,
        &io_ranges.diff_args,
        &io_ranges.diff_range,
        &io_ranges.log_range,
    )?;
    crate::model::sort_files_tree_order(&mut files);

    let view = View {
        repo_name: repo_name.clone(),
        repo_root: top.to_string(),
        branch,
        upstream: base_ref.clone(),
        title: view_ranges.title,
        cmd: view_ranges.cmd,
        commits_label: view_ranges.commits_label,
        foot: view_ranges.foot,
        commits,
        files,
        theme: crate::config::load().theme,
    };

    let summary = match target {
        DiffTarget::Range(_) => base_ref.clone(),
        DiffTarget::Base(_) => format!("{base_ref}..working"),
        DiffTarget::Merge(_) => format!("to merge into {base_ref}"),
        DiffTarget::Unpushed if fallback_to_main => format!("{base_ref}..working"),
        DiffTarget::Unpushed => legacy_unpushed_commit_label(view.commits.len()),
        DiffTarget::Last(count) => format!(
            "last {}",
            legacy_count_label(count.get() as usize, "commit")
        ),
    };
    Ok((view, summary))
}

struct DiffBase {
    ref_name: String,
    is_upstream: bool,
}

fn unpushed_or_main_base(top: &str) -> anyhow::Result<DiffBase> {
    match git::upstream(top) {
        Ok(upstream) => Ok(DiffBase {
            ref_name: upstream,
            is_upstream: true,
        }),
        Err(upstream_error) => {
            git::verify_commit(top, "main").map_err(|_| upstream_error)?;
            eprintln!("diff-preview: no upstream; falling back to main");
            Ok(DiffBase {
                ref_name: "main".to_string(),
                is_upstream: false,
            })
        }
    }
}

fn range_fast_path(top: &str, target: &DiffTarget) -> anyhow::Result<Option<PathBuf>> {
    let Some((kind, base_sha, head_sha)) = resolved_range(top, target) else {
        return Ok(None); // worktree mode or unresolved ⇒ no fast-path
    };
    let canonical = std::fs::canonicalize(top).unwrap_or_else(|_| PathBuf::from(top));
    let repo_id = gtl_store::repo_id(crate::git::root_commit(top).as_deref(), &canonical);
    let store_root = gtl_platform::paths::store_root()?;
    gtl_store::lookup_by_range(&store_root, &repo_id, kind, &base_sha, &head_sha)
}

// ! Returns (kind, base_sha, head_sha) only for pure commit ranges; None for
// ! worktree (Hash) mode. Mirrors build_view's range selection but skips assemble.
fn resolved_range(top: &str, target: &DiffTarget) -> Option<(gtl_store::DiffKind, String, String)> {
    let diff_range = match target {
        DiffTarget::Range(r) => {
            crate::commands::ranges(r, crate::commands::Mode::ExactRange).diff_range
        }
        DiffTarget::Last(n) => {
            crate::commands::ranges(
                &format!("HEAD~{n}..HEAD"),
                crate::commands::Mode::ExactRange,
            )
            .diff_range
        }
        DiffTarget::Merge(b) => crate::commands::ranges(b, crate::commands::Mode::Merge).diff_range,
        DiffTarget::Unpushed => {
            let base = unpushed_or_main_base(top).ok()?;
            if !base.is_upstream {
                return None; // falls back to Hash (worktree) mode
            }
            crate::commands::ranges(&base.ref_name, crate::commands::Mode::Unpushed).diff_range
        }
        DiffTarget::Base(_) => return None, // Hash mode ⇒ worktree
    };
    let kind = gtl_store::DiffKind::from_diff_range(&diff_range);
    if kind == gtl_store::DiffKind::WorkTree {
        return None;
    }
    let base = crate::git::resolve_sha(top, range_base(&diff_range)).ok()?;
    let head = crate::git::resolve_sha(top, diff_range.rsplit("..").next()?).ok()?;
    Some((kind, base, head))
}

fn verify_exact_range(repo: impl AsRef<Path>, range: &str) -> anyhow::Result<()> {
    let Some((start, end)) = range.split_once("..") else {
        anyhow::bail!("range must use <start>..<end>");
    };
    if start.trim().is_empty() || end.trim().is_empty() {
        anyhow::bail!("range must use <start>..<end>");
    }
    git::verify_commit(repo.as_ref(), start)?;
    git::verify_commit(repo, end)?;
    Ok(())
}
