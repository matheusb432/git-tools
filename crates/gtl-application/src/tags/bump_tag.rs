//! Revalidates and applies one exact tag-bump proposal.

use gtl_models::{
    failure::ErrorMeta,
    git::{GitRevision, TagName},
};

use super::{
    BumpLevel, DryRunTagBumpOk, TagBumpPreview,
    add_tag::{self, AddTag},
    dry_run_tag_bump,
    git_command_error::GitCommandError,
    outcome::{TagActionOutcome, TagOperationProgress},
    push_tags,
};
use crate::ports::{GitClient, UserSettingsLoadError, UserSettingsReader};

/// Result of applying one tag-bump proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BumpTagOk {
    Applied {
        tag: TagName,
        outcome: TagActionOutcome,
    },
    Rejected {
        detail: String,
    },
}

/// Reports a failure outside the proposal's own decision while applying it.
#[derive(Debug, thiserror::Error, ErrorMeta)]
#[non_exhaustive]
pub enum BumpTagError {
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error("{source}")]
    #[meta(private(Internal))]
    Unexpected {
        progress: TagOperationProgress,
        #[source]
        source: anyhow::Error,
    },
}

/// Recomputes the current proposal, compares it with the displayed proposal, and applies it.
#[cqrsy::command]
pub fn execute(
    preview: TagBumpPreview,
    git: &impl GitClient,
    settings: &impl UserSettingsReader,
) -> Result<BumpTagOk, BumpTagError> {
    let current = match dry_run_tag_bump::execute_resolved(
        dry_run_tag_bump::DryRunResolvedTagBump {
            repo_root: preview.repo_path.clone(),
            pattern: Some(preview.pattern.clone()),
            level: BumpLevel::Slot(preview.slot),
            message: preview.message.clone(),
            push: preview.push,
        },
        git,
        settings,
    ) {
        Ok(DryRunTagBumpOk::Ready(current)) => *current,
        Ok(DryRunTagBumpOk::Rejected { .. }) => return Ok(stale_preview()),
        Err(dry_run_tag_bump::DryRunTagBumpError::Settings(error)) => {
            return Err(BumpTagError::Settings(error));
        }
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
    let created = add_tag::create_at(
        AddTag {
            repo_path: preview.repo_path.clone(),
            tag: tag.clone(),
            message: preview.message,
        },
        &GitRevision::from(&preview.target_id),
        git,
    );
    let outcome = match created {
        Ok(outcome) if outcome.is_failed() => outcome,
        Ok(outcome) if preview.push => {
            match push_tags::push_named(
                git,
                &preview.repo_path,
                std::slice::from_ref(&tag),
                &outcome,
            ) {
                Ok(outcome) => outcome,
                Err(error) => return finish_git_error(error, tag),
            }
        }
        Ok(outcome) => outcome,
        Err(error) => return finish_git_error(error, tag),
    };

    Ok(BumpTagOk::Applied { tag, outcome })
}

fn finish_git_error(error: GitCommandError, tag: TagName) -> Result<BumpTagOk, BumpTagError> {
    match error {
        GitCommandError::Rejected { detail, progress } => Ok(BumpTagOk::Applied {
            tag,
            outcome: TagActionOutcome::failed_with_progress(detail, *progress),
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
