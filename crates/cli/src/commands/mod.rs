use std::path::{Path, PathBuf};

use crate::model::{Cmd, Foot};

pub mod diff;
pub mod diff_subrepos;
mod discover;
pub mod managed;
pub mod merge_diff;
pub mod prune;
pub mod squash_local;
pub mod squash_preview;
pub mod sw;
pub mod sync;
pub mod tag;
pub mod up_subrepos;
pub mod worktree;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Unpushed,
    Hash,
    Merge,
    ExactRange,
}

#[derive(Debug, Clone)]
pub struct Ranges {
    pub diff_args: Vec<String>,
    pub diff_range: String,
    pub log_range: String,
    pub title: String,
    pub cmd: Cmd,
    pub commits_label: String,
    pub foot: Foot,
}

pub fn ranges(base: &str, mode: Mode) -> Ranges {
    match mode {
        Mode::Unpushed => {
            let range = format!("{base}..HEAD");
            Ranges {
                diff_args: vec!["diff".to_string(), range.clone()],
                diff_range: range.clone(),
                log_range: range.clone(),
                title: "diff".to_string(),
                cmd: Cmd {
                    lead: "git diff ".to_string(),
                    range: range.clone(),
                    trail: String::new(),
                },
                commits_label: "# unpushed commits".to_string(),
                foot: Foot {
                    cmd: format!("git diff {range}"),
                    note: "# unpushed work — read-only preview".to_string(),
                },
            }
        }
        Mode::Hash => {
            let log_range = format!("{base}..HEAD");
            Ranges {
                diff_args: vec!["diff".to_string(), base.to_string()],
                diff_range: base.to_string(),
                log_range,
                title: "diff".to_string(),
                cmd: Cmd {
                    lead: "git diff ".to_string(),
                    range: base.to_string(),
                    trail: String::new(),
                },
                commits_label: format!("# commits since {base}"),
                foot: Foot {
                    cmd: format!("git diff {base}"),
                    note: "# base → working tree — read-only preview".to_string(),
                },
            }
        }
        Mode::Merge => {
            let diff_range = format!("{base}...HEAD");
            let log_range = format!("{base}..HEAD");
            Ranges {
                diff_args: vec!["diff".to_string(), diff_range.clone()],
                diff_range: diff_range.clone(),
                log_range,
                title: "merge-diff".to_string(),
                cmd: Cmd {
                    lead: "git diff ".to_string(),
                    range: diff_range.clone(),
                    trail: String::new(),
                },
                commits_label: "# commits to merge".to_string(),
                foot: Foot {
                    cmd: format!("git diff {diff_range}"),
                    note: "# merge preview — read-only".to_string(),
                },
            }
        }
        Mode::ExactRange => Ranges {
            diff_args: vec!["diff".to_string(), base.to_string()],
            diff_range: base.to_string(),
            log_range: base.to_string(),
            title: "diff".to_string(),
            cmd: Cmd {
                lead: "git diff ".to_string(),
                range: base.to_string(),
                trail: String::new(),
            },
            commits_label: "# commits in range".to_string(),
            foot: Foot {
                cmd: format!("git diff {base}"),
                note: "# commit range — read-only preview".to_string(),
            },
        },
    }
}

fn repo_name(top: impl AsRef<Path>) -> String {
    top.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}

/// Everything the store needs to record one rendered artifact.
pub struct ArtifactMeta {
    pub repo_root: String,
    pub repo_name: String,
    pub kind: infra::store::DiffKind,
    pub base_sha: String,
    pub head_sha: String,
    pub range_label: String,
    pub head_committed_at: String,
    pub title: String,
}

/// Place a rendered artifact in the central store (never the repo). Returns the
/// artifact path. Idempotent on identical content.
pub(crate) fn store_artifact(meta: &ArtifactMeta, html: &str) -> anyhow::Result<PathBuf> {
    let store_root = gtl_platform::paths::store_root()?;
    let canonical =
        std::fs::canonicalize(&meta.repo_root).unwrap_or_else(|_| PathBuf::from(&meta.repo_root));
    let root_commit = crate::git::root_commit(&meta.repo_root);
    let repo_id = infra::store::repo_id(root_commit.as_deref(), &canonical);
    let sidecar = infra::store::Sidecar {
        repo_id: repo_id.clone(),
        repo_name: meta.repo_name.clone(),
        repo_root: meta.repo_root.clone(),
        kind: meta.kind,
        base_sha: meta.base_sha.clone(),
        head_sha: meta.head_sha.clone(),
        range_label: meta.range_label.clone(),
        head_committed_at: meta.head_committed_at.clone(),
        generated_at: jiff::Timestamp::now().to_string(),
        title: meta.title.clone(),
        byte_size: html.len() as u64,
    };
    Ok(infra::store::place(&store_root, &repo_id, html, &sidecar)?.path)
}

/// Open an artifact according to the user's `viewer` config. `app` spawns the
/// desktop viewer detached on the `diff://` url; if the app or a display is
/// missing it degrades to the browser. Best-effort — never fails the command.
pub(crate) fn open_artifact(path: &Path) {
    use crate::viewer::{
        ViewerAction, diff_url_from_path, is_no_open, resolve_viewer_action, resolve_viewer_bin,
    };
    let has_display =
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some();
    let no_open = is_no_open(std::env::var("GIT_TOOLS_NO_OPEN").ok().as_deref());
    let action = resolve_viewer_action(crate::config::load().diff.viewer, has_display, no_open);
    match action {
        ViewerAction::Nothing => {}
        ViewerAction::Browser => gtl_platform::open_in_browser(path),
        ViewerAction::SpawnApp => {
            match (resolve_viewer_bin(), diff_url_from_path(path)) {
                (Some(bin), Some(url)) => {
                    if gtl_platform::spawn_detached(&bin, &[url.as_str()]).is_err() {
                        gtl_platform::open_in_browser(path); // spawn failed → browser
                    }
                }
                // Viewer not installed or unparseable path → browser fallback.
                _ => gtl_platform::open_in_browser(path),
            }
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn legacy_count_label(count: usize, noun: &str) -> String {
    format!("{count} {noun}(s)")
}

fn legacy_unpushed_commit_label(count: usize) -> String {
    format!("{count} unpushed commit(s)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_mode_uses_three_dot_diff_and_two_dot_log() {
        let ranges = ranges("main", Mode::Merge);

        assert_eq!(ranges.diff_args, vec!["diff", "main...HEAD"]);
        assert_eq!(ranges.diff_range, "main...HEAD");
        assert_eq!(ranges.log_range, "main..HEAD");
    }

    #[test]
    fn hash_mode_diffs_base_to_working_tree_and_logs_base_to_head() {
        let ranges = ranges("abc123", Mode::Hash);

        assert_eq!(ranges.diff_args, vec!["diff", "abc123"]);
        assert_eq!(ranges.diff_range, "abc123");
        assert_eq!(ranges.log_range, "abc123..HEAD");
        assert_eq!(ranges.title, "diff");
        assert_eq!(ranges.commits_label, "# commits since abc123");
        assert_eq!(ranges.foot.cmd, "git diff abc123");
    }

    #[test]
    fn unpushed_mode_uses_upstream_to_head_for_diff_and_log() {
        let ranges = ranges("origin/main", Mode::Unpushed);

        assert_eq!(ranges.diff_args, vec!["diff", "origin/main..HEAD"]);
        assert_eq!(ranges.diff_range, "origin/main..HEAD");
        assert_eq!(ranges.log_range, "origin/main..HEAD");
        assert_eq!(ranges.title, "diff");
        assert_eq!(ranges.commits_label, "# unpushed commits");
        assert_eq!(ranges.foot.cmd, "git diff origin/main..HEAD");
    }

    #[test]
    fn exact_range_mode_uses_range_for_diff_and_log() {
        let ranges = ranges("abc123..def456", Mode::ExactRange);

        assert_eq!(ranges.diff_args, vec!["diff", "abc123..def456"]);
        assert_eq!(ranges.diff_range, "abc123..def456");
        assert_eq!(ranges.log_range, "abc123..def456");
        assert_eq!(ranges.commits_label, "# commits in range");
    }

    #[test]
    fn merge_mode_labels_three_dot_merge_preview() {
        let ranges = ranges("main", Mode::Merge);

        assert_eq!(ranges.title, "merge-diff");
        assert_eq!(ranges.commits_label, "# commits to merge");
        assert_eq!(ranges.foot.cmd, "git diff main...HEAD");
    }

    #[test]
    fn legacy_count_label_keeps_node_literal_plural_marker() {
        assert_eq!(legacy_count_label(1, "file"), "1 file(s)");
        assert_eq!(legacy_count_label(2, "commit"), "2 commit(s)");
        assert_eq!(legacy_unpushed_commit_label(1), "1 unpushed commit(s)");
    }
}
