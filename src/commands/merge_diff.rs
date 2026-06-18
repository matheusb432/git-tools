use std::path::{Path, PathBuf};

use crate::commands::{Mode, output_file, plural, ranges, repo_name};
use crate::diff::parse_diff;
use crate::git;
use crate::model::View;
use crate::open::open_file;
use crate::render::build_html;

pub const DEFAULT_BASE: &str = "main";

pub fn run(
    repo: impl AsRef<Path>,
    monorepo: impl AsRef<Path>,
    base: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let top = git::top_level(repo)?;
    let branch = git::current_branch(&top)?;
    let repo_name = repo_name(&top);
    let base = base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE);

    git::verify_commit(&top, base)?;
    let ranges = ranges(base, Mode::Merge);
    let commits = git::log_commits(&top, &ranges.log_range)?;
    let mut files = parse_diff(&git::diff_raw(&top, &ranges.diff_args)?);
    let file_commits = git::file_commit_map(&top, &ranges.log_range)?;
    git::attach_commits(&mut files, &file_commits);

    let view = View {
        repo_name: repo_name.clone(),
        repo_root: top.clone(),
        branch,
        upstream: base.to_string(),
        title: ranges.title,
        cmd: ranges.cmd,
        commits_label: ranges.commits_label,
        foot: ranges.foot,
        commits,
        files,
    };

    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let html = build_html(&view);
    let out_file = output_file(monorepo, &format!("merge-diff-{repo_name}.html"), &html)?;

    println!(
        "merge-diff: {commit_count} commit{} to merge into {base}, {file_count} file{}",
        plural(commit_count),
        plural(file_count)
    );
    println!("wrote {}", out_file.display());
    open_file(&out_file);
    Ok(out_file)
}
