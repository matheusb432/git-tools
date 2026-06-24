use std::path::{Path, PathBuf};

use crate::{
    commands::{Mode, legacy_count_label, legacy_unpushed_commit_label, ranges, repo_name},
    git,
    model::{Cmd, Foot, View},
    render::build_html,
};

pub fn run(repo: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let top = git::top_level(repo)?;
    let upstream = git::upstream(&top)?;
    let branch = git::current_branch(&top)?;
    let repo_name = repo_name(&top);
    let ranges = ranges(&upstream, Mode::Unpushed);

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
        theme: None,
    };

    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let html = build_html(&view);

    let meta = super::ArtifactMeta {
        repo_root: top.to_string(),
        repo_name: repo_name.clone(),
        // ! WorkTree by design: squash-preview is base→working-tree, not a commit range,
        // ! so it is intentionally excluded from range-dedup in the store.
        kind: gtl_store::DiffKind::WorkTree,
        base_sha: String::new(),
        head_sha: crate::git::resolve_sha(&top, "HEAD").unwrap_or_default(),
        range_label: ranges.log_range.clone(),
        head_committed_at: crate::git::committed_at(&top, "HEAD"),
        title: "squash-preview".to_string(),
    };
    let out_file = super::store_artifact(&meta, &html)?;

    println!(
        "squash-preview: {}, {}",
        legacy_unpushed_commit_label(commit_count),
        legacy_count_label(file_count, "file")
    );
    println!("wrote {}", out_file.display());
    super::open_artifact(&out_file);
    Ok(out_file)
}

fn collapse_note(commit_count: usize) -> String {
    if commit_count == 1 {
        "# would collapse this commit into one — read-only preview".to_string()
    } else {
        format!("# would collapse these {commit_count} commits into one — read-only preview")
    }
}
