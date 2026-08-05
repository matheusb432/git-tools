//! Gathers the current Git state for one exact tag-bump proposal.

use std::path::{Path, PathBuf};

use gtl_models::tags::Tag;

use super::{
    BumpLevel,
    logic::{bump::decide_tag_version, git_command_error::GitCommandError},
};
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
    let decision = match decide_tag_version(local_tags.keys().map(String::as_str), query.level) {
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

#[cfg(test)]
mod tests {
    use super::{DryRunTagBump, DryRunTagBumpOk, execute};
    use crate::{tags::BumpLevel, testing::ScriptedGitClient};

    #[test]
    fn pushed_preview_uses_local_tags_without_querying_origin() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo"),
            ScriptedGitClient::applied("commit-a\t\t\tv1.2.3\t"),
            ScriptedGitClient::applied("main"),
            ScriptedGitClient::applied("target-sha"),
        ]);

        let result = execute(
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
        assert_eq!(preview.next_tag, "v1.2.4");
    }
}
