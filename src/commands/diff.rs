use std::path::{Path, PathBuf};

use crate::cli::DiffTarget;
use crate::commands::{
    Mode, legacy_count_label, legacy_unpushed_commit_label, output_file, ranges, repo_name,
};
use crate::diff::parse_diff;
use crate::git;
use crate::model::View;
use crate::open::open_file;
use crate::render::build_html;

pub fn run(target: &DiffTarget) -> anyhow::Result<PathBuf> {
    let top = git::top_level(".")?;
    render(&top, &top, target)
}

pub(crate) fn render(
    top: &str,
    monorepo: impl AsRef<Path>,
    target: &DiffTarget,
) -> anyhow::Result<PathBuf> {
    let (view, summary) = build_view(top, target)?;
    let file_count = view.files.len();
    let repo_name = view.repo_name.clone();
    let html = build_html(&view);
    let out_file = output_file(monorepo, &format!("diff-preview-{repo_name}.html"), &html)?;

    println!(
        "diff-preview: {summary}, {}",
        legacy_count_label(file_count, "file")
    );
    println!("wrote {}", out_file.display());
    open_file(&out_file);
    Ok(out_file)
}

pub(crate) fn build_view(top: &str, target: &DiffTarget) -> anyhow::Result<(View, String)> {
    let branch = git::current_branch(top)?;
    let repo_name = repo_name(top);

    let (base_ref, io_ranges, view_ranges) = match target {
        DiffTarget::Range(range) => {
            verify_exact_range(top, range)?;
            (
                range.clone(),
                ranges(range, Mode::ExactRange),
                ranges(range, Mode::ExactRange),
            )
        }
        DiffTarget::Base(base) => {
            git::verify_commit(top, base)?;
            let short = git::short_ref(top, base)?;
            (
                short.clone(),
                ranges(base, Mode::Hash),
                ranges(&short, Mode::Hash),
            )
        }
        DiffTarget::Unpushed => {
            let upstream = git::upstream(top)?;
            (
                upstream.clone(),
                ranges(&upstream, Mode::Unpushed),
                ranges(&upstream, Mode::Unpushed),
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
            )
        }
    };

    let commits = git::log_commits(top, &io_ranges.log_range)?;
    let mut files = parse_diff(&git::diff_raw(top, &io_ranges.diff_args)?);
    let file_commits = git::file_commit_map(top, &io_ranges.log_range)?;
    git::attach_commits(&mut files, &file_commits);

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
    };

    let summary = match target {
        DiffTarget::Range(_) => base_ref.clone(),
        DiffTarget::Base(_) => format!("{base_ref}..working"),
        DiffTarget::Unpushed => legacy_unpushed_commit_label(view.commits.len()),
        DiffTarget::Last(count) => format!(
            "last {}",
            legacy_count_label(count.get() as usize, "commit")
        ),
    };
    Ok((view, summary))
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
