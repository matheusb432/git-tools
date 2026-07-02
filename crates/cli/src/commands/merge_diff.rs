use std::path::{Path, PathBuf};

use crate::{
    commands::{Mode, plural, ranges, repo_name},
    git,
    model::View,
    render::build_html,
};

pub const DEFAULT_BASE: &str = "main";

pub fn run(repo: impl AsRef<Path>, base: Option<&str>) -> anyhow::Result<PathBuf> {
    let top = git::top_level(repo)?;
    let branch = git::current_branch(&top)?;
    let repo_name = repo_name(&top);
    let base = base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE);

    git::verify_commit(&top, base)?;
    let ranges = ranges(base, Mode::Merge);
    let crate::diff::DiffData { commits, files } = crate::diff::assemble(
        &top,
        &ranges.diff_args,
        &ranges.diff_range,
        &ranges.log_range,
    )?;

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
        theme: None,
    };

    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let html = build_html(&view);

    let meta = super::ArtifactMeta {
        repo_root: top.to_string(),
        repo_name: repo_name.clone(),
        kind: infra::store::DiffKind::from_diff_range(&ranges.diff_range),
        base_sha: crate::git::resolve_sha(&top, base).unwrap_or_default(),
        head_sha: crate::git::resolve_sha(&top, "HEAD").unwrap_or_default(),
        range_label: ranges.diff_range.clone(),
        head_committed_at: crate::git::committed_at(&top, "HEAD"),
        title: "merge-diff".to_string(),
    };
    let out_file = super::store_artifact(&meta, &html)?;

    println!(
        "merge-diff: {commit_count} commit{} to merge into {base}, {file_count} file{}",
        plural(commit_count),
        plural(file_count)
    );
    println!("wrote {}", out_file.display());
    super::open_artifact(&out_file);
    Ok(out_file)
}
