//! Shared structured Git worktree values and porcelain loading.

use std::path::Path;

use crate::ports::GitRunner;

/// Describes one entry from `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// Absolute or caller-facing path reported by Git.
    pub path: String,
    /// Full `HEAD` object identifier reported by Git.
    pub head: String,
    /// Local branch name without the `refs/heads/` prefix.
    pub branch: Option<String>,
    /// Whether Git reports a detached `HEAD`.
    pub detached: bool,
    /// Whether Git reports a bare worktree.
    pub bare: bool,
    /// Optional reason Git reports the worktree as locked.
    pub locked: Option<String>,
    /// Optional reason Git reports the worktree as prunable.
    pub prunable: Option<String>,
}

impl Worktree {
    fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            head: String::new(),
            branch: None,
            detached: false,
            bare: false,
            locked: None,
            prunable: None,
        }
    }
}

pub(super) enum WorktreeLoad {
    Listed(Vec<Worktree>),
    Rejected(String),
}

pub(super) fn load(git: &impl GitRunner, repo: &Path) -> anyhow::Result<WorktreeLoad> {
    let output = git.run(repo, &["worktree", "list", "--porcelain"])?;
    if output.exit_code == 0 {
        Ok(WorktreeLoad::Listed(parse(&output.stdout)))
    } else {
        Ok(WorktreeLoad::Rejected(format!(
            "git worktree list failed (exit {})",
            output.exit_code
        )))
    }
}

fn parse(raw: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<Worktree> = None;

    for line in raw.lines() {
        if line.is_empty() {
            if let Some(worktree) = current.take() {
                worktrees.push(worktree);
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(worktree) = current.replace(Worktree::new(path)) {
                worktrees.push(worktree);
            }
            continue;
        }

        let Some(worktree) = current.as_mut() else {
            continue;
        };

        if let Some(head) = line.strip_prefix("HEAD ") {
            worktree.head = head.to_string();
        } else if let Some(branch) = line.strip_prefix("branch ") {
            worktree.branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_string(),
            );
        } else if line == "detached" {
            worktree.detached = true;
        } else if line == "bare" {
            worktree.bare = true;
        } else if let Some(reason) = line.strip_prefix("locked") {
            worktree.locked = Some(reason.trim_start().to_string());
        } else if let Some(reason) = line.strip_prefix("prunable") {
            worktree.prunable = Some(reason.trim_start().to_string());
        }
    }

    if let Some(worktree) = current {
        worktrees.push(worktree);
    }

    worktrees
}
