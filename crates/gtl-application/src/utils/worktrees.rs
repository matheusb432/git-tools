//! Worktree parsing helpers for test Git clients.

use gtl_models::{
    diffs::CommitId,
    git::BranchName,
    paths::RepositoryRoot,
    worktrees::{Worktree, WorktreeCheckout, WorktreeKind},
};

use super::try_commit_id_fixture;

struct WorktreeBuilder {
    path: String,
    id: Option<CommitId>,
    kind: Option<WorktreeKind>,
    locked: Option<String>,
    prunable: Option<String>,
}

impl WorktreeBuilder {
    fn finish(self) -> anyhow::Result<Worktree> {
        Ok(Worktree::new(
            RepositoryRoot::try_new(self.path.into())?,
            self.id
                .ok_or_else(|| anyhow::anyhow!("scripted worktree omitted its HEAD commit ID"))?,
            self.kind.ok_or_else(|| {
                anyhow::anyhow!("scripted worktree omitted its checkout or bare state")
            })?,
            self.locked,
            self.prunable,
        ))
    }

    fn set_kind(&mut self, kind: WorktreeKind) -> anyhow::Result<()> {
        if self.kind.replace(kind).is_some() {
            anyhow::bail!("scripted worktree reported conflicting checkout states");
        }
        Ok(())
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
                kind: None,
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
            let branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);
            worktree.set_kind(WorktreeKind::Checkout(WorktreeCheckout::Branch(
                BranchName::try_new(branch.to_owned())?,
            )))?;
        } else if line == "detached" {
            worktree.set_kind(WorktreeKind::Checkout(WorktreeCheckout::Detached))?;
        } else if line == "bare" {
            worktree.set_kind(WorktreeKind::Bare)?;
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
