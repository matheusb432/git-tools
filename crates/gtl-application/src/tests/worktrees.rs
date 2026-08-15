use gtl_models::{diffs::CommitId, worktrees::Worktree};

use super::try_commit_id_fixture;

struct WorktreeBuilder {
    path: String,
    id: Option<CommitId>,
    branch: Option<String>,
    detached: bool,
    bare: bool,
    locked: Option<String>,
    prunable: Option<String>,
}

impl WorktreeBuilder {
    fn finish(self) -> anyhow::Result<Worktree> {
        Ok(Worktree {
            path: self.path,
            id: self
                .id
                .ok_or_else(|| anyhow::anyhow!("scripted worktree omitted its HEAD commit ID"))?,
            branch: self.branch,
            detached: self.detached,
            bare: self.bare,
            locked: self.locked,
            prunable: self.prunable,
        })
    }
}

pub(super) fn parse(raw: &str) -> anyhow::Result<Vec<Worktree>> {
    let mut worktrees = Vec::new();
    let mut current: Option<WorktreeBuilder> = None;

    for line in raw.lines() {
        if line.is_empty() {
            if let Some(worktree) = current.take() {
                worktrees.push(worktree.finish()?);
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(worktree) = current.replace(WorktreeBuilder {
                path: path.to_owned(),
                id: None,
                branch: None,
                detached: false,
                bare: false,
                locked: None,
                prunable: None,
            }) {
                worktrees.push(worktree.finish()?);
            }
            continue;
        }

        let Some(worktree) = current.as_mut() else {
            continue;
        };

        if let Some(raw_id) = line.strip_prefix("HEAD ") {
            worktree.id = Some(try_commit_id_fixture(raw_id)?);
        } else if let Some(branch) = line.strip_prefix("branch ") {
            worktree.branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_owned(),
            );
        } else if line == "detached" {
            worktree.detached = true;
        } else if line == "bare" {
            worktree.bare = true;
        } else if let Some(reason) = line.strip_prefix("locked") {
            worktree.locked = Some(reason.trim_start().to_owned());
        } else if let Some(reason) = line.strip_prefix("prunable") {
            worktree.prunable = Some(reason.trim_start().to_owned());
        }
    }

    if let Some(worktree) = current {
        worktrees.push(worktree.finish()?);
    }

    Ok(worktrees)
}
