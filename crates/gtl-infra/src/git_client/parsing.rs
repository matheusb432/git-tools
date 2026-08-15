use std::collections::BTreeMap;

use gtl_application::ports::GitWorkingTree;
use gtl_models::{
    diffs::{CommitId, CommitIdError},
    managed::working_tree::CommitFile,
    tags::Tag,
    worktrees::Worktree,
};

pub(super) fn parse_working_tree(raw: &str) -> GitWorkingTree {
    let mut tree = GitWorkingTree::default();
    for line in raw.lines().filter(|line| !line.is_empty()) {
        let bytes = line.as_bytes();
        if bytes.len() < 2 {
            continue;
        }
        tree.files.push(CommitFile {
            status: line.get(0..2).unwrap_or("").trim().to_string(),
            path: line.get(3..).unwrap_or("").to_string(),
        });
        let (index, worktree) = (bytes[0], bytes[1]);
        if index == b'?' && worktree == b'?' {
            tree.unprepared += 1;
        } else {
            tree.staged += usize::from(index != b' ');
            tree.unprepared += usize::from(worktree != b' ');
        }
    }
    tree
}

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
                .ok_or_else(|| anyhow::anyhow!("Git worktree output omitted its HEAD commit ID"))?,
            branch: self.branch,
            detached: self.detached,
            bare: self.bare,
            locked: self.locked,
            prunable: self.prunable,
        })
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
                path: path.to_string(),
                id: None,
                branch: None,
                detached: false,
                bare: false,
                locked: None,
                prunable: None,
            }) {
                worktrees.push(worktree.finish()?);
            }
        } else if let Some(worktree) = current.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                worktree.id = Some(head.try_into()?);
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
    }
    if let Some(worktree) = current {
        worktrees.push(worktree.finish()?);
    }
    Ok(worktrees)
}

pub(super) fn parse_local_tags(output: &str) -> Result<BTreeMap<String, Tag>, CommitIdError> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let mut fields = line.splitn(4, '\t');
            let object = fields.next()?.to_string();
            let peeled_commit = fields.next()?.to_string();
            let name = fields.next()?.to_string();
            let message_and_date = fields.next().unwrap_or_default();
            let (message, created_at) = message_and_date
                .rsplit_once('\t')
                .map_or((message_and_date, None), |(message, date)| {
                    (message, date.parse().ok())
                });
            let annotated = !peeled_commit.is_empty();
            let commit = if annotated {
                peeled_commit
            } else {
                object.clone()
            };
            Some(commit.try_into().map(|commit: CommitId| {
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
                (name, tag)
            }))
        })
        .collect()
}

pub(super) fn parse_remote_tags(output: &str) -> BTreeMap<String, String> {
    output
        .lines()
        .filter_map(|line| {
            let (object, reference) = line.split_once('\t')?;
            let name = reference.strip_prefix("refs/tags/")?;
            (!name.ends_with("^{}")).then(|| (name.to_string(), object.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use gtl_models::diffs::CommitIdError;

    use super::{parse_local_tags, parse_working_tree};

    #[test]
    fn porcelain_status_becomes_a_semantic_working_tree() {
        let tree = parse_working_tree("M  staged\n M changed\nMM both\n?? new\n");

        assert_eq!(tree.files.len(), 4);
        assert_eq!(tree.staged, 2);
        assert_eq!(tree.unprepared, 3);
    }

    #[test]
    fn local_tags_reject_an_invalid_resolved_commit_id() {
        assert_eq!(
            parse_local_tags("invalid\t\tv1.0.0\t\t100"),
            Err(CommitIdError)
        );
    }
}
