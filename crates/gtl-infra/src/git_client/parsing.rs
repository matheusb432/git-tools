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
}

pub(super) fn parse_worktrees(raw: &str) -> anyhow::Result<Vec<Worktree>> {
    let mut worktrees = Vec::new();
    let mut current: Option<WorktreeBuilder> = None;
    for line in raw.lines() {
        if line.is_empty() {
            if let Some(worktree) = current.take() {
                worktrees.push(worktree.finish()?);
            }
        } else if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(worktree) = current.replace(WorktreeBuilder {
                path: RepositoryRoot::try_new(path.into())?,
                id: None,
                kind: None,
                locked: None,
                prunable: None,
            }) {
                worktrees.push(worktree.finish()?);
            }
        } else if let Some(worktree) = current.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                worktree.id = Some(head.try_into()?);
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
                worktree.locked = Some(reason.trim_start().to_string());
            } else if let Some(reason) = line.strip_prefix("prunable") {
                worktree.prunable = Some(reason.trim_start().to_string());
            }
        }
    }
    if let Some(worktree) = current {
        worktrees.push(worktree.finish()?);
    }
    Ok(worktrees)
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
                .expect_err("malformed Git epoch must fail at the decode boundary");

        assert!(error.to_string().contains("invalid creation epoch"));
    }
}
