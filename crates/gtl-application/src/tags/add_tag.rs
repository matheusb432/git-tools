//! Creates one annotated tag without publishing it.

use gtl_models::{
    git::{GitRevision, TagName},
    paths::RepositoryRoot,
};

use super::{
    git_command_error::GitCommandError,
    outcome::{TagActionOutcome, TagOperationProgress},
};
use crate::ports::GitClient;

/// Requests creation of one annotated tag in a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddTag {
    pub repo_path: RepositoryRoot,
    pub tag: TagName,
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
            Ok(TagActionOutcome::failed_with_progress(detail, *progress))
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
    create_at(command, &GitRevision::head(), git)
}

pub(super) fn create_at(
    command: AddTag,
    revision: &GitRevision,
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
        crate::ports::GitEffect::Applied(()) => Ok(TagActionOutcome::created(
            format!("created tag {tag}"),
            &tag,
        )),
        crate::ports::GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git tag add failed for {tag}: {detail}"
        ))),
    }
}

fn validate(command: &AddTag) -> Option<TagActionOutcome> {
    if command.message.trim().is_empty() {
        return Some(TagActionOutcome::failed("tag message is required"));
    }
    None
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{AddTag, validate};
    use crate::{
        tags::{
            add_tag,
            outcome::{TagActionOutcome, TagActionStatus, TagRemotePushProgress},
        },
        utils::ScriptedGitClient,
    };

    #[test]
    fn validation_preserves_required_message_detail() {
        assert_eq!(
            validate(&AddTag {
                repo_path: crate::utils::repository_root("/repo"),
                tag: crate::utils::tag_name("v1.0.0"),
                message: "\n".into(),
            }),
            Some(TagActionOutcome::failed("tag message is required"))
        );
    }

    #[test]
    fn transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = add_tag::execute(
            AddTag {
                repo_path: crate::utils::repository_root("/repo"),
                tag: crate::utils::tag_name("v1.0.0"),
                message: "release".into(),
            },
            &git,
        )
        .unwrap_err();

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
            add_tag::execute(
                AddTag {
                    repo_path: crate::utils::repository_root("/repo"),
                    tag: crate::utils::tag_name("v1.0.0"),
                    message: "release".into(),
                },
                &git,
            )
            .unwrap(),
            TagActionOutcome::failed("git tag add failed for v1.0.0: fatal: tag already exists")
        );
    }

    #[test]
    fn successful_add_reports_the_created_ref() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);

        let outcome = add_tag::execute(
            AddTag {
                repo_path: crate::utils::repository_root("/repo"),
                tag: crate::utils::tag_name("v1.0.0"),
                message: "release".into(),
            },
            &git,
        )
        .unwrap();

        assert_eq!(outcome.status(), TagActionStatus::Created);
        assert_eq!(
            outcome.progress().created_refs,
            [gtl_models::git::GitRefName::for_tag(
                &crate::utils::tag_name("v1.0.0")
            )]
            .into_iter()
            .collect()
        );
        assert_eq!(
            outcome.progress().remote_push,
            TagRemotePushProgress::NotStarted
        );
    }
}
