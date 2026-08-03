//! Gathers the current Git state for one exact tag-bump proposal.

use std::path::{Path, PathBuf};

use gtl_models::tags::Tag;

use super::{BumpLevel, git_command_error::GitCommandError, logic::decide_tag_version};
use crate::ports::{GitClient, GitEffect};

/// Requests a tag-bump proposal without changing Git state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunTagBump {
    pub repo_path: PathBuf,
    pub level: BumpLevel,
    pub message: String,
    pub push: bool,
}

/// The exact mutation displayed before a tag bump is confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagBumpPreview {
    pub repo_path: PathBuf,
    pub branch: String,
    pub target_sha: String,
    pub level: BumpLevel,
    pub base_tag: String,
    pub next_tag: String,
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

    let query = DryRunTagBump {
        repo_path,
        level,
        message,
        push,
    };
    match read_preview(&query, git) {
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
    query: &DryRunTagBump,
    git: &impl GitClient,
) -> Result<Result<TagBumpPreview, String>, GitCommandError> {
    let Some(repo_path) = git.discover_top(&query.repo_path)? else {
        return Ok(Err(format!(
            "{} is not inside a Git repository",
            query.repo_path.display()
        )));
    };
    let local_tags = local_tags(git, &repo_path)?;
    let remote_tags = query
        .push
        .then(|| remote_tags(git, &repo_path))
        .transpose()?;
    let tag_names = local_tags.keys().map(String::as_str).chain(
        remote_tags
            .iter()
            .flat_map(|tags| tags.keys().map(String::as_str)),
    );
    let decision = match decide_tag_version(tag_names, query.level) {
        Ok(decision) => decision,
        Err(rejection) => return Ok(Err(rejection.to_string())),
    };
    let branch = git.current_branch(&repo_path)?;
    let target_sha = git.resolve_sha(&repo_path, "HEAD")?;

    Ok(Ok(TagBumpPreview {
        repo_path,
        branch,
        target_sha,
        level: query.level,
        base_tag: decision.base_tag,
        next_tag: decision.next_tag,
        message: query.message.clone(),
        push: query.push,
    }))
}

fn local_tags(
    git: &impl GitClient,
    repo_path: &Path,
) -> Result<std::collections::BTreeMap<String, Tag>, GitCommandError> {
    match git.local_tags(repo_path)? {
        GitEffect::Applied(tags) => Ok(tags),
        GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git for-each-ref failed: {detail}"
        ))),
    }
}

fn remote_tags(
    git: &impl GitClient,
    repo_path: &Path,
) -> Result<std::collections::BTreeMap<String, String>, GitCommandError> {
    match git.remote_tags(repo_path, "origin")? {
        GitEffect::Applied(tags) => Ok(tags),
        GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git ls-remote failed: {detail}"
        ))),
    }
}
