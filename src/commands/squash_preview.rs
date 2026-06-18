use std::path::{Path, PathBuf};

use crate::commands::{
    Mode, legacy_count_label, legacy_unpushed_commit_label, output_file, ranges, repo_name,
};
use crate::diff::parse_diff;
use crate::git;
use crate::model::{Cmd, Foot, View};
use crate::open::open_file;
use crate::render::build_html;

pub fn run(repo: impl AsRef<Path>, monorepo: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let top = git::top_level(repo)?;
    let upstream = git::upstream(&top)?;
    let branch = git::current_branch(&top)?;
    let repo_name = repo_name(&top);
    let ranges = ranges(&upstream, Mode::Unpushed);

    let commits = git::log_commits(&top, &ranges.log_range)?;
    let mut files = parse_diff(&git::diff_raw(&top, &ranges.diff_args)?);
    let file_commits = git::file_commit_map(&top, &ranges.log_range)?;
    git::attach_commits(&mut files, &file_commits);

    let view = View {
        repo_name: repo_name.clone(),
        repo_root: top.clone(),
        branch,
        upstream: upstream.clone(),
        title: "squash-preview".to_string(),
        cmd: Cmd {
            lead: "git log ".to_string(),
            range: ranges.log_range.clone(),
            trail: " --stat".to_string(),
        },
        commits_label: "# commits — collapse into 1".to_string(),
        foot: Foot {
            cmd: "squash-local".to_string(),
            note: collapse_note(commits.len()),
        },
        commits,
        files,
    };

    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let html = build_html(&view);
    let out_file = output_file(monorepo, &format!("squash-preview-{repo_name}.html"), &html)?;

    println!(
        "squash-preview: {}, {}",
        legacy_unpushed_commit_label(commit_count),
        legacy_count_label(file_count, "file")
    );
    println!("wrote {}", out_file.display());
    open_file(&out_file);
    Ok(out_file)
}

fn collapse_note(commit_count: usize) -> String {
    if commit_count == 1 {
        "# would collapse this commit into one — read-only preview".to_string()
    } else {
        format!("# would collapse these {commit_count} commits into one — read-only preview")
    }
}
