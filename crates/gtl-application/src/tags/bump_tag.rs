//! Revalidates and applies one exact tag-bump proposal.

use super::{
    DryRunTagBump, DryRunTagBumpOk, TagBumpPreview,
    add::{self, AddTag},
    dry_run_tag_bump,
    git_command_error::GitCommandError,
    outcome::{TagActionOutcome, TagActionStatus, TagOperationProgress},
    push,
};
use crate::ports::GitClient;

/// Requests application of the exact proposal displayed to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BumpTag {
    pub preview: TagBumpPreview,
}

/// Result of applying one tag-bump proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BumpTagOk {
    Applied {
        tag: String,
        outcome: TagActionOutcome,
    },
    Rejected {
        detail: String,
    },
}

/// Reports an unexpected Git transport failure while applying a proposal.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BumpTagError {
    #[error("{source}")]
    Unexpected {
        progress: TagOperationProgress,
        #[source]
        source: anyhow::Error,
    },
}

/// Recomputes the current proposal, compares it with the displayed proposal, and applies it.
#[cqrsy::command]
pub fn execute(command: BumpTag, git: &impl GitClient) -> Result<BumpTagOk, BumpTagError> {
    let BumpTag { preview } = command;
    let current = match dry_run_tag_bump::execute(
        DryRunTagBump {
            repo_path: preview.repo_path.clone(),
            level: preview.level,
            message: preview.message.clone(),
            push: preview.push,
        },
        git,
    ) {
        Ok(DryRunTagBumpOk::Ready(current)) => current,
        Ok(DryRunTagBumpOk::Rejected { .. }) => return Ok(stale_preview()),
        Err(dry_run_tag_bump::DryRunTagBumpError::Unexpected { source }) => {
            return Err(BumpTagError::Unexpected {
                progress: TagOperationProgress::default(),
                source,
            });
        }
    };
    if current != preview {
        return Ok(stale_preview());
    }

    let tag = preview.next_tag.clone();
    let created = add::create_at(
        AddTag {
            repo_path: preview.repo_path.clone(),
            tag: tag.clone(),
            message: preview.message,
        },
        &preview.target_sha,
        git,
    );
    let outcome = match created {
        Ok(outcome) if outcome.status == TagActionStatus::Failed => outcome,
        Ok(outcome) if preview.push => {
            match push::push_named(git, &preview.repo_path, std::slice::from_ref(&tag), outcome) {
                Ok(outcome) => outcome,
                Err(error) => return finish_git_error(error, tag),
            }
        }
        Ok(outcome) => outcome,
        Err(error) => return finish_git_error(error, tag),
    };

    Ok(BumpTagOk::Applied { tag, outcome })
}

fn finish_git_error(error: GitCommandError, tag: String) -> Result<BumpTagOk, BumpTagError> {
    match error {
        GitCommandError::Rejected { detail, progress } => Ok(BumpTagOk::Applied {
            tag,
            outcome: TagActionOutcome::failed(detail).with_progress(*progress),
        }),
        GitCommandError::Transport { source, progress } => Err(BumpTagError::Unexpected {
            progress: *progress,
            source,
        }),
    }
}

fn stale_preview() -> BumpTagOk {
    BumpTagOk::Rejected {
        detail: "tag bump preview is stale; run dry-run again".into(),
    }
}
