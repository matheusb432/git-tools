//! Attaches and publishes a lightweight label for an existing tag.

use std::path::{Path, PathBuf};

use super::{
    git_command_error::GitCommandError,
    outcome::{TagActionOutcome, TagActionStatus, TagOperationProgress},
    push,
};
use crate::ports::GitRunner;

/// Requests a lightweight label that resolves through an existing tag to its commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelTag {
    pub repo: PathBuf,
    pub tag: String,
    pub label: String,
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
pub fn execute(command: LabelTag, git: &impl GitRunner) -> Result<TagActionOutcome, LabelTagError> {
    match label(command, git) {
        Ok(outcome) => Ok(outcome),
        Err(GitCommandError::Rejected { detail, progress }) => {
            Ok(TagActionOutcome::failed(detail).with_progress(*progress))
        }
        Err(GitCommandError::Transport { source, progress }) => Err(LabelTagError::Unexpected {
            progress: *progress,
            source,
        }),
    }
}

fn label(command: LabelTag, git: &impl GitRunner) -> Result<TagActionOutcome, GitCommandError> {
    if let Some(failure) = validate(&command) {
        return Ok(failure);
    }
    let LabelTag { repo, tag, label } = command;
    create(git, &repo, &tag, &label)?;

    let created = TagActionOutcome::new(TagActionStatus::Created, format!("created tag {label}"))
        .with_progress(TagOperationProgress::created(label.clone()));
    push::push_named(git, &repo, std::slice::from_ref(&label), created)
}

pub(super) fn create(
    git: &impl GitRunner,
    repo: &Path,
    target: &str,
    label: &str,
) -> Result<(), GitCommandError> {
    let peeled = format!("{target}^{{}}");
    let output = git.run(repo, &["tag", label, &peeled])?;
    if output.exit_code == 0 {
        Ok(())
    } else {
        Err(GitCommandError::rejected(
            output.fail_detail(&format!("git tag label failed for {label}")),
        ))
    }
}

fn validate(command: &LabelTag) -> Option<TagActionOutcome> {
    if command.tag.trim().is_empty() {
        return Some(TagActionOutcome::failed("tag name is required"));
    }
    if command.label.trim().is_empty() {
        return Some(TagActionOutcome::failed("label is required"));
    }
    None
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{LabelTag, execute, validate};
    use crate::{
        tags::outcome::{TagActionOutcome, TagActionStatus, TagOperationProgress},
        testing::FakeGitRunner,
    };

    #[test]
    fn validation_preserves_required_tag_detail() {
        assert_eq!(
            validate(&LabelTag {
                repo: ".".into(),
                tag: String::new(),
                label: "stable".into(),
            }),
            Some(TagActionOutcome {
                status: TagActionStatus::Failed,
                detail: "tag name is required".into(),
                progress: TagOperationProgress::default(),
            })
        );
    }

    #[test]
    fn validation_preserves_required_label_detail() {
        assert_eq!(
            validate(&LabelTag {
                repo: ".".into(),
                tag: "v1.0.0".into(),
                label: "  ".into(),
            }),
            Some(TagActionOutcome {
                status: TagActionStatus::Failed,
                detail: "label is required".into(),
                progress: TagOperationProgress::default(),
            })
        );
    }

    #[test]
    fn creation_transport_failure_remains_an_error_with_its_source() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(
            LabelTag {
                repo: ".".into(),
                tag: "v1.0.0".into(),
                label: "stable".into(),
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
    fn creation_nonzero_exit_remains_the_exact_closed_failure() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::exit_err("fatal: invalid target", 128)]);

        assert_eq!(
            execute(
                LabelTag {
                    repo: ".".into(),
                    tag: "v1.0.0".into(),
                    label: "stable".into(),
                },
                &git,
            )
            .expect("a Git rejection is a closed action failure"),
            TagActionOutcome {
                status: TagActionStatus::Failed,
                detail: "git tag label failed for stable: fatal: invalid target".into(),
                progress: TagOperationProgress::default(),
            }
        );
    }
}
