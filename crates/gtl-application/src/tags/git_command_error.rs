use super::outcome::TagOperationProgress;

#[derive(Debug)]
pub(super) enum GitCommandError {
    Rejected {
        detail: String,
        progress: Box<TagOperationProgress>,
    },
    Transport {
        source: anyhow::Error,
        progress: Box<TagOperationProgress>,
    },
}

impl GitCommandError {
    pub(super) fn rejected(detail: impl Into<String>) -> Self {
        Self::Rejected {
            detail: detail.into(),
            progress: Box::default(),
        }
    }

    pub(super) fn with_prior_progress(mut self, prior: TagOperationProgress) -> Self {
        match &mut self {
            Self::Rejected { progress, .. } | Self::Transport { progress, .. } => {
                progress.merge(prior);
            }
        }
        self
    }
}

impl From<anyhow::Error> for GitCommandError {
    fn from(source: anyhow::Error) -> Self {
        Self::Transport {
            source,
            progress: Box::default(),
        }
    }
}
