//! Creates one annotated tag without publishing it.

use std::path::PathBuf;

use super::{
    git_command_error::GitCommandError,
    outcome::{TagActionOutcome, TagActionStatus, TagOperationProgress},
};
use crate::ports::GitClient;

/// Requests creation of one annotated tag in a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddTag {
    pub repo_path: PathBuf,
    pub tag: String,
    pub message: String,
}

/// Reports an unexpected Git transport failure while adding a tag.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AddTagError {
    /// Git could not be started or its output could not be collected.
    #[error("{source}")]
    Unexpected {
        progress: TagOperationProgress,
        #[source]
        source: anyhow::Error,
    },
}

/// Creates the requested annotated tag.
///
/// # Errors
///
/// Returns [`AddTagError`] when Git cannot be executed.
#[cqrsy::command]
pub fn execute(command: AddTag, git: &impl GitClient) -> Result<TagActionOutcome, AddTagError> {
    match create(command, git) {
        Ok(outcome) => Ok(outcome),
        Err(GitCommandError::Rejected { detail, progress }) => {
            Ok(TagActionOutcome::failed(detail).with_progress(*progress))
        }
        Err(GitCommandError::Transport { source, progress }) => Err(AddTagError::Unexpected {
            progress: *progress,
            source,
        }),
    }
}

pub(super) fn create(
    command: AddTag,
    git: &impl GitClient,
) -> Result<TagActionOutcome, GitCommandError> {
    create_at(command, "HEAD", git)
}

pub(super) fn create_at(
    command: AddTag,
    revision: &str,
    git: &impl GitClient,
) -> Result<TagActionOutcome, GitCommandError> {
    if let Some(failure) = validate(&command) {
        return Ok(failure);
    }

    let AddTag {
        repo_path,
        tag,
        message,
    } = command;
    match git.create_annotated_tag_at(&repo_path, &tag, revision, &message)? {
        crate::ports::GitEffect::Applied(()) => Ok(TagActionOutcome::new(
            TagActionStatus::Created,
            format!("created tag {tag}"),
        )
        .with_progress(TagOperationProgress::created(tag))),
        crate::ports::GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git tag add failed for {tag}: {detail}"
        ))),
    }
}

fn validate(command: &AddTag) -> Option<TagActionOutcome> {
    if command.tag.trim().is_empty() {
        return Some(TagActionOutcome::failed("tag name is required"));
    }
    if command.message.trim().is_empty() {
        return Some(TagActionOutcome::failed("tag message is required"));
    }
    None
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{AddTag, execute, validate};
    use crate::{
        tags::outcome::{
            TagActionOutcome, TagActionStatus, TagOperationProgress, TagRemotePushProgress,
        },
        testing::ScriptedGitClient,
    };

    #[test]
    fn validation_preserves_required_tag_detail() {
        assert_eq!(
            validate(&AddTag {
                repo_path: ".".into(),
                tag: "  ".into(),
                message: "release".into(),
            }),
            Some(TagActionOutcome {
                status: TagActionStatus::Failed,
                detail: "tag name is required".into(),
                progress: TagOperationProgress::default(),
            })
        );
    }

    #[test]
    fn validation_preserves_required_message_detail() {
        assert_eq!(
            validate(&AddTag {
                repo_path: ".".into(),
                tag: "v1.0.0".into(),
                message: "\n".into(),
            }),
            Some(TagActionOutcome {
                status: TagActionStatus::Failed,
                detail: "tag message is required".into(),
                progress: TagOperationProgress::default(),
            })
        );
    }

    #[test]
    fn transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(
            AddTag {
                repo_path: ".".into(),
                tag: "v1.0.0".into(),
                message: "release".into(),
            },
            &git,
        )
        .expect_err("transport failure must remain an error");

        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }

    #[test]
    fn nonzero_exit_remains_the_exact_closed_failure() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::rejected(
            "fatal: tag already exists",
        )]);

        assert_eq!(
            execute(
                AddTag {
                    repo_path: ".".into(),
                    tag: "v1.0.0".into(),
                    message: "release".into(),
                },
                &git,
            )
            .expect("a Git rejection is a closed action failure"),
            TagActionOutcome {
                status: TagActionStatus::Failed,
                detail: "git tag add failed for v1.0.0: fatal: tag already exists".into(),
                progress: TagOperationProgress::default(),
            }
        );
    }

    #[test]
    fn successful_add_reports_the_created_ref() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);

        let outcome = execute(
            AddTag {
                repo_path: ".".into(),
                tag: "v1.0.0".into(),
                message: "release".into(),
            },
            &git,
        )
        .expect("tag creation succeeds");

        assert_eq!(outcome.status, TagActionStatus::Created);
        assert_eq!(outcome.progress.created_refs, vec!["refs/tags/v1.0.0"]);
        assert_eq!(
            outcome.progress.remote_push,
            TagRemotePushProgress::NotStarted
        );
    }
}
