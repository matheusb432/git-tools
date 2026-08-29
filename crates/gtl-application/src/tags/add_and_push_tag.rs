//! Creates an annotated tag, optionally labels it, and publishes only those new refs.

use gtl_models::{git::TagName, paths::RepositoryRoot};

use super::{
    add_tag::{self, AddTag},
    git_command_error::GitCommandError,
    label_tag,
    outcome::{TagActionOutcome, TagOperationProgress},
    push_tags,
};
use crate::ports::GitClient;

/// Requests creation and publication of an annotated tag and optional lightweight label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddAndPushTag {
    pub repo_path: RepositoryRoot,
    pub tag: TagName,
    pub message: String,
    pub label: Option<TagName>,
}

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
) -> Result<TagActionOutcome, AddAndPushTagError> {
    match add_and_push(command, git) {
        Ok(outcome) => Ok(outcome),
        Err(GitCommandError::Rejected { detail, progress }) => {
            Ok(TagActionOutcome::failed_with_progress(detail, *progress))
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
        repo_path: command.repo_path.clone(),
        tag: command.tag.clone(),
        message: command.message,
    };
    let created = add_tag::create(add, git)?;
    if created.is_failed() {
        return Ok(created);
    }

    let mut names = vec![command.tag.clone()];
    let mut created = created;
    if let Some(label_name) = command.label {
        label_tag::create(git, &command.repo_path, &command.tag, &label_name)
            .map_err(|error| error.with_prior_progress(created.progress().clone()))?;
        created = created.with_created_ref(&label_name);
        names.push(label_name);
    }

    push_tags::push_named(git, &command.repo_path, &names, &created)
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{AddAndPushTag, AddAndPushTagError};
    use crate::{
        tags::{TagRemotePushProgress, add_and_push_tag},
        utils::ScriptedGitClient,
    };

    #[test]
    fn transitive_label_transport_failure_uses_the_operation_error_surface() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error: AddAndPushTagError = add_and_push_tag::execute(
            AddAndPushTag {
                repo_path: crate::utils::repository_root("/repo"),
                tag: crate::utils::tag_name("v1.0.0"),
                message: "release".into(),
                label: Some(crate::utils::tag_name("stable")),
            },
            &git,
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
        let AddAndPushTagError::Unexpected { progress, .. } = error;
        assert_eq!(
            progress.created_refs,
            [gtl_models::git::GitRefName::for_tag(
                &crate::utils::tag_name("v1.0.0")
            )]
            .into_iter()
            .collect()
        );
        assert_eq!(progress.remote_push, TagRemotePushProgress::NotStarted);
    }
}
