use std::path::{Path, PathBuf};

use crate::commands::{
    Mode, legacy_count_label, legacy_unpushed_commit_label, output_file, ranges, repo_name,
};
use crate::diff::parse_diff;
use crate::git;
use crate::model::View;
use crate::open::open_file;
use crate::render::build_html;

pub fn run(
    repo: impl AsRef<Path>,
    monorepo: impl AsRef<Path>,
    base: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let top = git::top_level(repo)?;
    let branch = git::current_branch(&top)?;
    let repo_name = repo_name(&top);

    let base = base.map(str::trim).filter(|base| !base.is_empty());
    let (base_ref, io_ranges, view_ranges) = if let Some(base) = base {
        git::verify_commit(&top, base)?;
        let short = git::short_ref(&top, base)?;
        (
            short.clone(),
            ranges(base, Mode::Hash),
            ranges(&short, Mode::Hash),
        )
    } else {
        let upstream = git::upstream(&top)?;
        (
            upstream.clone(),
            ranges(&upstream, Mode::Unpushed),
            ranges(&upstream, Mode::Unpushed),
        )
    };

    let commits = git::log_commits(&top, &io_ranges.log_range)?;
    let mut files = parse_diff(&git::diff_raw(&top, &io_ranges.diff_args)?);
    let file_commits = git::file_commit_map(&top, &io_ranges.log_range)?;
    git::attach_commits(&mut files, &file_commits);

    let view = View {
        repo_name: repo_name.clone(),
        branch,
        upstream: base_ref.clone(),
        title: view_ranges.title,
        cmd: view_ranges.cmd,
        commits_label: view_ranges.commits_label,
        foot: view_ranges.foot,
        commits,
        files,
    };

    let summary = if base.is_some() {
        format!("{}..working", base_ref)
    } else {
        legacy_unpushed_commit_label(view.commits.len())
    };
    let file_count = view.files.len();
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
