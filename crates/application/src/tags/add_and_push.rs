//! Creates an annotated tag, optionally labels it, and publishes only those new refs.

use std::path::PathBuf;

use super::{
    add::{self, AddTag},
    git_command_error::GitCommandError,
    label,
    outcome::{TagActionOutcome, TagActionStatus, TagOperationProgress},
    push,
};
use crate::ports::GitClient;

/// Requests creation and publication of an annotated tag and optional lightweight label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddAndPushTag {
    pub repo: PathBuf,
    pub tag: String,
    pub message: String,
    pub label: Option<String>,
}

pub type AddAndPushTagOk = TagActionOutcome;

/// Reports an unexpected Git transport failure while adding and publishing a tag.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AddAndPushTagError {
    /// Git could not be started or its output could not be collected.
    #[error("{source}")]
    Unexpected {
        progress: TagOperationProgress,
        #[source]
        source: anyhow::Error,
    },
}

/// Creates the requested refs and publishes only those refs with explicit refspecs.
///
/// # Errors
///
/// Returns [`AddAndPushTagError`] when Git cannot be executed.
#[cqrsy::command]
pub fn execute(
    command: AddAndPushTag,
    git: &impl GitClient,
) -> Result<AddAndPushTagOk, AddAndPushTagError> {
    match add_and_push(command, git) {
        Ok(outcome) => Ok(outcome),
        Err(GitCommandError::Rejected { detail, progress }) => {
            Ok(TagActionOutcome::failed(detail).with_progress(*progress))
        }
        Err(GitCommandError::Transport { source, progress }) => {
            Err(AddAndPushTagError::Unexpected {
                progress: *progress,
                source,
            })
        }
    }
}

fn add_and_push(
    command: AddAndPushTag,
    git: &impl GitClient,
) -> Result<TagActionOutcome, GitCommandError> {
    let add = AddTag {
        repo: command.repo.clone(),
        tag: command.tag.clone(),
        message: command.message,
    };
    let created = add::create(add, git)?;
    if created.status == TagActionStatus::Failed {
        return Ok(created);
    }

    let mut names = vec![command.tag.clone()];
    let mut created = created;
    if let Some(label_name) = command.label {
        label::create(git, &command.repo, &command.tag, &label_name)
            .map_err(|error| error.with_prior_progress(created.progress.clone()))?;
        created.detail = format!("{}\ncreated tag {label_name}", created.detail);
        created.progress.record_created(label_name.clone());
        names.push(label_name);
    }

    push::push_named(git, &command.repo, &names, created)
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{AddAndPushTag, AddAndPushTagError, execute};
    use crate::{tags::TagRemotePushProgress, testing::ScriptedGitClient};

    #[test]
    fn transitive_label_transport_failure_uses_the_operation_error_surface() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error: AddAndPushTagError = execute(
            AddAndPushTag {
                repo: ".".into(),
                tag: "v1.0.0".into(),
                message: "release".into(),
                label: Some("stable".into()),
            },
            &git,
        )
        .expect_err("transport failure must remain an error");

        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
        let AddAndPushTagError::Unexpected { progress, .. } = error;
        assert_eq!(progress.created_refs, vec!["refs/tags/v1.0.0"]);
        assert_eq!(progress.remote_push, TagRemotePushProgress::NotStarted);
    }
}
