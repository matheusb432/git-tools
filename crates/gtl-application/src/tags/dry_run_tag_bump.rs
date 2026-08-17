//! Gathers the current Git state for one exact tag-bump proposal.

use std::path::PathBuf;

use gtl_models::{
    diffs::CommitId,
    git::{GitHead, GitRevision, TagName},
    paths::RepositoryRoot,
    tags::Tag,
};

use super::{BumpLevel, git_command_error::GitCommandError, version::decide_tag_version};
use crate::ports::{GitClient, GitEffect};

/// Requests a tag-bump proposal without changing Git state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunTagBump {
    pub repo_path: PathBuf,
    pub level: BumpLevel,
    pub message: String,
    pub push: bool,
}

/// Requests a tag-bump proposal for an already-resolved repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunResolvedTagBump {
    pub repo_root: RepositoryRoot,
    pub level: BumpLevel,
    pub message: String,
    pub push: bool,
}

/// The exact mutation displayed before a tag bump is confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagBumpPreview {
    pub repo_path: RepositoryRoot,
    pub branch: GitHead,
    pub target_id: CommitId,
    pub level: BumpLevel,
    pub base_tag: TagName,
    pub next_tag: TagName,
    pub message: String,
    pub push: bool,
}

/// Result of gathering a tag-bump proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DryRunTagBumpOk {
    Ready(TagBumpPreview),
    Rejected { detail: String },
}

/// Reports an unexpected Git transport failure while gathering a proposal.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DryRunTagBumpError {
    #[error("{source}")]
    Unexpected {
        #[source]
        source: anyhow::Error,
    },
}

/// Gathers the repository state needed to describe one exact tag mutation.
#[cqrsy::query]
pub fn execute(
    query: DryRunTagBump,
    git: &impl GitClient,
) -> Result<DryRunTagBumpOk, DryRunTagBumpError> {
    let DryRunTagBump {
        repo_path,
        level,
        message,
        push,
    } = query;
    if !repo_path.is_absolute() {
        return Ok(DryRunTagBumpOk::Rejected {
            detail: "tag bump repository path must be absolute".into(),
        });
    }
    if message.trim().is_empty() {
        return Ok(DryRunTagBumpOk::Rejected {
            detail: "tag message is required".into(),
        });
    }

    let Some(repo_root) = git
        .discover_top(&repo_path)
        .map_err(|source| DryRunTagBumpError::Unexpected { source })?
    else {
        return Ok(DryRunTagBumpOk::Rejected {
            detail: format!("{} is not inside a Git repository", repo_path.display()),
        });
    };
    evaluate_preview(&repo_root, level, &message, push, git)
}

/// Gathers a proposal without discarding a resolved repository identity.
pub fn execute_resolved(
    query: DryRunResolvedTagBump,
    git: &impl GitClient,
) -> Result<DryRunTagBumpOk, DryRunTagBumpError> {
    let DryRunResolvedTagBump {
        repo_root,
        level,
        message,
        push,
    } = query;
    if message.trim().is_empty() {
        return Ok(DryRunTagBumpOk::Rejected {
            detail: "tag message is required".into(),
        });
    }
    let Some(repo_root) = git
        .discover_top(repo_root.as_ref())
        .map_err(|source| DryRunTagBumpError::Unexpected { source })?
    else {
        return Ok(DryRunTagBumpOk::Rejected {
            detail: format!("{} is not inside a Git repository", repo_root.display()),
        });
    };
    evaluate_preview(&repo_root, level, &message, push, git)
}

fn evaluate_preview(
    repo_root: &RepositoryRoot,
    level: BumpLevel,
    message: &str,
    push: bool,
    git: &impl GitClient,
) -> Result<DryRunTagBumpOk, DryRunTagBumpError> {
    match read_preview(repo_root, level, message, push, git) {
        Ok(Ok(preview)) => Ok(DryRunTagBumpOk::Ready(preview)),
        Ok(Err(detail)) | Err(GitCommandError::Rejected { detail, .. }) => {
            Ok(DryRunTagBumpOk::Rejected { detail })
        }
        Err(GitCommandError::Transport { source, .. }) => {
            Err(DryRunTagBumpError::Unexpected { source })
        }
    }
}

fn read_preview(
    repo_path: &RepositoryRoot,
    level: BumpLevel,
    message: &str,
    push: bool,
    git: &impl GitClient,
) -> Result<Result<TagBumpPreview, String>, GitCommandError> {
    let local_tags = local_tags(git, repo_path)?;
    let decision = match decide_tag_version(local_tags.keys().map(AsRef::as_ref), level) {
        Ok(decision) => decision,
        Err(rejection) => return Ok(Err(rejection.to_string())),
    };
    let branch = git.current_branch(repo_path)?;
    let target_id = git.resolve_commit_id(repo_path, &GitRevision::head())?;

    Ok(Ok(TagBumpPreview {
        repo_path: repo_path.clone(),
        branch,
        target_id,
        level,
        base_tag: decision.base_tag,
        next_tag: decision.next_tag,
        message: message.to_owned(),
        push,
    }))
}

fn local_tags(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
) -> Result<std::collections::BTreeMap<TagName, Tag>, GitCommandError> {
    match git.local_tags(repo_path)? {
        GitEffect::Applied(tags) => Ok(tags),
        GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git for-each-ref failed: {detail}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{DryRunTagBump, DryRunTagBumpOk};
    use crate::{
        tags::{BumpLevel, dry_run_tag_bump},
        utils::ScriptedGitClient,
    };

    #[test]
    fn pushed_preview_uses_local_tags_without_querying_origin() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo"),
            ScriptedGitClient::applied("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t\tv1.2.3\t\t100"),
            ScriptedGitClient::applied("main"),
            ScriptedGitClient::applied("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        ]);

        let result = dry_run_tag_bump::execute(
            DryRunTagBump {
                repo_path: "/repo".into(),
                level: BumpLevel::Patch,
                message: "release".into(),
                push: true,
            },
            &git,
        )
        .expect("local tag preview should succeed without origin access");

        let DryRunTagBumpOk::Ready(preview) = result else {
            panic!("expected a ready tag bump preview");
        };
        assert_eq!(preview.next_tag.as_ref(), "v1.2.4");
    }
}
