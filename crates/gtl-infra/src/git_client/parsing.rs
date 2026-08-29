use std::collections::BTreeMap;

use anyhow::Context as _;
use gtl_models::{
    diffs::CommitId,
    git::{BranchName, GitObjectId, TagName},
    paths::RepositoryRoot,
    tags::Tag,
    timestamps::MachineTimestamp,
    worktrees::{Worktree, WorktreeCheckout, WorktreeKind},
};

struct WorktreeBuilder {
    path: RepositoryRoot,
    id: Option<CommitId>,
    kind: Option<WorktreeKind>,
    locked: Option<String>,
    prunable: Option<String>,
}

impl WorktreeBuilder {
    fn new(path: &str) -> anyhow::Result<Self> {
        Ok(Self {
            path: RepositoryRoot::try_new(path.into())?,
            id: None,
            kind: None,
            locked: None,
            prunable: None,
        })
    }

    fn finish(self) -> anyhow::Result<Worktree> {
        Ok(Worktree::new(
            self.path,
            self.id
                .ok_or_else(|| anyhow::anyhow!("Git worktree output omitted its HEAD commit ID"))?,
            self.kind.ok_or_else(|| {
                anyhow::anyhow!("Git worktree output omitted its checkout or bare state")
            })?,
            self.locked,
            self.prunable,
        ))
    }

    fn set_kind(&mut self, kind: WorktreeKind) -> anyhow::Result<()> {
        if self.kind.replace(kind).is_some() {
            anyhow::bail!("Git worktree output reported conflicting checkout states");
        }
        Ok(())
    }

    fn apply_line(&mut self, line: &str) -> anyhow::Result<()> {
        if let Some(head) = line.strip_prefix("HEAD ") {
            self.id = Some(head.try_into()?);
            return Ok(());
        }
        if let Some(branch) = line.strip_prefix("branch ") {
            let branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);
            return self.set_kind(WorktreeKind::Checkout(WorktreeCheckout::Branch(
                BranchName::try_new(branch.to_owned())?,
            )));
        }
        if line == "detached" {
            return self.set_kind(WorktreeKind::Checkout(WorktreeCheckout::Detached));
        }
        if line == "bare" {
            return self.set_kind(WorktreeKind::Bare);
        }
        if let Some(reason) = line.strip_prefix("locked") {
            self.locked = Some(reason.trim_start().to_string());
            return Ok(());
        }
        if let Some(reason) = line.strip_prefix("prunable") {
            self.prunable = Some(reason.trim_start().to_string());
        }
        Ok(())
    }
}

pub(super) fn parse_worktrees(raw: &str) -> anyhow::Result<Vec<Worktree>> {
    let mut worktrees = Vec::new();
    let mut current: Option<WorktreeBuilder> = None;
    for line in raw.lines() {
        parse_worktree_line(line, &mut current, &mut worktrees)?;
    }
    finish_current_worktree(&mut current, &mut worktrees)?;
    Ok(worktrees)
}

fn parse_worktree_line(
    line: &str,
    current: &mut Option<WorktreeBuilder>,
    worktrees: &mut Vec<Worktree>,
) -> anyhow::Result<()> {
    if line.is_empty() {
        return finish_current_worktree(current, worktrees);
    }
    if let Some(path) = line.strip_prefix("worktree ") {
        finish_current_worktree(current, worktrees)?;
        *current = Some(WorktreeBuilder::new(path)?);
        return Ok(());
    }
    if let Some(worktree) = current.as_mut() {
        worktree.apply_line(line)?;
    }
    Ok(())
}

fn finish_current_worktree(
    current: &mut Option<WorktreeBuilder>,
    worktrees: &mut Vec<Worktree>,
) -> anyhow::Result<()> {
    let Some(worktree) = current.take() else {
        return Ok(());
    };
    worktrees.push(worktree.finish()?);
    Ok(())
}

pub(super) fn parse_local_tags(output: &str) -> anyhow::Result<BTreeMap<TagName, Tag>> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.splitn(4, '\t');
            let object = GitObjectId::try_new(fields.next().unwrap_or_default().to_owned())?;
            let peeled_commit = fields.next().unwrap_or_default().to_owned();
            let name = TagName::try_new(fields.next().unwrap_or_default().to_owned())?;
            let message_and_date = fields.next().unwrap_or_default();
            let (message, created_at) = match message_and_date.rsplit_once('\t') {
                Some((message, "")) => (message, None),
                Some((message, raw)) => {
                    let seconds = raw
                        .parse::<i64>()
                        .context("Git tag has an invalid creation epoch")?;
                    let timestamp = MachineTimestamp::from_unix_seconds(seconds)
                        .context("Git tag creation epoch is out of range")?;
                    (message, Some(timestamp))
                }
                None => (message_and_date, None),
            };
            let annotated = !peeled_commit.is_empty();
            let commit = if annotated {
                peeled_commit
            } else {
                object.to_string()
            };
            let commit: CommitId = commit.try_into()?;
            let tag = if annotated {
                Tag::annotated(
                    name.clone(),
                    object,
                    commit,
                    created_at,
                    Some(message.trim().to_string()).filter(|message| !message.is_empty()),
                )
            } else {
                Tag::lightweight(name.clone(), commit, created_at)
            };
            Ok((name, tag))
        })
        .collect()
}

pub(super) fn parse_remote_tags(output: &str) -> anyhow::Result<BTreeMap<TagName, GitObjectId>> {
    output
        .lines()
        .filter_map(|line| {
            let (object, reference) = line.split_once('\t')?;
            let name = reference.strip_prefix("refs/tags/")?;
            (!name.ends_with("^{}")).then_some((name, object))
        })
        .map(|(name, object)| {
            Ok((
                TagName::try_new(name.to_owned())?,
                GitObjectId::try_new(object.to_owned())?,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_local_tags;

    #[test]
    fn local_tags_reject_an_invalid_resolved_commit_id() {
        assert!(parse_local_tags("invalid\t\tv1.0.0\t\t100").is_err());
    }

    #[test]
    fn local_tags_reject_a_malformed_creation_epoch() {
        let error =
            parse_local_tags("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t\tv1.0.0\t\tnot-an-epoch")
                .unwrap_err();

        assert!(error.to_string().contains("invalid creation epoch"));
    }
}
