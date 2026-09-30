//! Attaches and publishes a lightweight label for an existing tag.

use gtl_models::{
    git::{GitRevision, TagName},
    paths::RepositoryRoot,
};

use super::{
    git_command_error::GitCommandError,
    outcome::{TagActionOutcome, TagOperationProgress},
    push_tags,
};
use crate::ports::GitClient;

/// Requests a lightweight label that resolves through an existing tag to its commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelTag {
    pub repo_path: RepositoryRoot,
    pub tag: TagName,
    pub label: TagName,
}

/// Reports an unexpected Git transport failure while labeling a tag.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LabelTagError {
    /// Git could not be started or its output could not be collected.
    #[error("{source}")]
    Unexpected {
        progress: TagOperationProgress,
        #[source]
        source: anyhow::Error,
    },
}

/// Creates the lightweight label and publishes it with an explicit tag refspec.
///
/// # Errors
///
/// Returns [`LabelTagError`] when Git cannot be executed.
#[cqrsy::command]
pub fn execute(command: LabelTag, git: &impl GitClient) -> Result<TagActionOutcome, LabelTagError> {
    match label(command, git) {
        Ok(outcome) => Ok(outcome),
        Err(GitCommandError::Rejected { detail, progress }) => {
            Ok(TagActionOutcome::failed_with_progress(detail, *progress))
        }
        Err(GitCommandError::Transport { source, progress }) => Err(LabelTagError::Unexpected {
            progress: *progress,
            source,
        }),
    }
}

fn label(command: LabelTag, git: &impl GitClient) -> Result<TagActionOutcome, GitCommandError> {
    let LabelTag {
        repo_path,
        tag,
        label,
    } = command;
    create(git, &repo_path, &tag, &label)?;

    let created = TagActionOutcome::created(format!("created tag {label}"), &label);
    push_tags::push_named(git, &repo_path, std::slice::from_ref(&label), &created)
}

pub(super) fn create(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
    target: &TagName,
    label: &TagName,
) -> Result<(), GitCommandError> {
    let target = GitRevision::from(target);
    match git.create_lightweight_tag(repo_path, label, &target)? {
        crate::ports::GitEffect::Applied(()) => Ok(()),
        crate::ports::GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git tag label failed for {label}: {detail}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::LabelTag;
    use crate::{
        tags::{label_tag, outcome::TagActionOutcome},
        utils::ScriptedGitClient,
    };

    #[test]
    fn creation_transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = label_tag::execute(
            LabelTag {
                repo_path: crate::utils::repository_root("//fixture.invalid/repositories/repo"),
                tag: crate::utils::tag_name("v1.0.0"),
                label: crate::utils::tag_name("stable"),
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
    fn creation_nonzero_exit_remains_the_exact_closed_failure() {
        let git =
            ScriptedGitClient::new(vec![ScriptedGitClient::rejected("fatal: invalid target")]);

        assert_eq!(
            label_tag::execute(
                LabelTag {
                    repo_path: crate::utils::repository_root("//fixture.invalid/repositories/repo"),
                    tag: crate::utils::tag_name("v1.0.0"),
                    label: crate::utils::tag_name("stable"),
                },
                &git,
            )
            .unwrap(),
            TagActionOutcome::failed("git tag label failed for stable: fatal: invalid target")
        );
    }
}
