/// Reports whether a remote tag push was not started, completed, or became indeterminate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TagRemotePushProgress {
    /// No remote push was attempted.
    #[default]
    NotStarted,
    /// Git confirmed the listed refs were pushed.
    Completed { pushed_refs: Vec<String> },
    /// Git was started for the listed refs but did not confirm the final remote state.
    Indeterminate { attempted_refs: Vec<String> },
}

/// Reports refs created and published before an outcome or error was reached.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagOperationProgress {
    pub created_refs: Vec<String>,
    pub remote_push: TagRemotePushProgress,
}

impl TagOperationProgress {
    pub(crate) fn created(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            created_refs: vec![local_ref(&name)],
            ..Self::default()
        }
    }

    pub(crate) fn record_created(&mut self, name: impl Into<String>) {
        push_unique(&mut self.created_refs, local_ref(&name.into()));
    }

    pub(crate) fn record_push_attempt(&mut self, names: &[String]) {
        self.remote_push = TagRemotePushProgress::Indeterminate {
            attempted_refs: names.iter().map(|name| local_ref(name)).collect(),
        };
    }

    pub(crate) fn record_push_completed(&mut self, names: &[String]) {
        self.remote_push = TagRemotePushProgress::Completed {
            pushed_refs: names.iter().map(|name| local_ref(name)).collect(),
        };
    }

    pub(crate) fn merge(&mut self, prior: Self) {
        for name in prior.created_refs {
            push_unique(&mut self.created_refs, name);
        }
        if self.remote_push == TagRemotePushProgress::NotStarted {
            self.remote_push = prior.remote_push;
        }
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn local_ref(name: &str) -> String {
    format!("refs/tags/{name}")
}

/// Classifies a tag creation or publication attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagActionStatus {
    /// An annotated tag was created locally.
    Created,
    /// Origin already held every requested tag object.
    Noop,
    /// One or more tag refs were published.
    Pushed,
    /// Validation or a Git operation failed.
    Failed,
}

/// Reports a closed tag action status and its exact user-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagActionOutcome {
    /// The closed action classification used for output and exit mapping.
    pub status: TagActionStatus,
    /// The exact user-facing detail preserved from the tag workflow.
    pub detail: String,
    /// The refs changed before this closed outcome was reached.
    pub progress: TagOperationProgress,
}

impl TagActionOutcome {
    pub(crate) fn new(status: TagActionStatus, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
            progress: TagOperationProgress::default(),
        }
    }

    pub(crate) fn with_progress(mut self, progress: TagOperationProgress) -> Self {
        self.progress = progress;
        self
    }

    pub(crate) fn failed(detail: impl Into<String>) -> Self {
        Self::new(TagActionStatus::Failed, detail)
    }

    pub(crate) fn with_created_detail(self, created_detail: String) -> Self {
        let progress = self.progress;
        match self.status {
            TagActionStatus::Pushed => Self::new(
                TagActionStatus::Pushed,
                format!("{created_detail}\n{}", self.detail),
            )
            .with_progress(progress),
            TagActionStatus::Noop => {
                Self::new(TagActionStatus::Noop, created_detail).with_progress(progress)
            }
            TagActionStatus::Created | TagActionStatus::Failed => Self {
                status: self.status,
                detail: self.detail,
                progress,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TagActionOutcome, TagActionStatus, TagOperationProgress};

    #[test]
    fn pushed_outcome_appends_to_the_multi_line_creation_detail() {
        assert_eq!(
            TagActionOutcome {
                status: TagActionStatus::Pushed,
                detail: "pushed 2 tags: v1.0.0, stable".into(),
                progress: TagOperationProgress::default(),
            }
            .with_created_detail("created tag v1.0.0\ncreated tag stable".into()),
            TagActionOutcome {
                status: TagActionStatus::Pushed,
                detail: "created tag v1.0.0\ncreated tag stable\npushed 2 tags: v1.0.0, stable"
                    .into(),
                progress: TagOperationProgress::default(),
            }
        );
    }

    #[test]
    fn failed_push_keeps_the_push_failure_detail() {
        let failure = TagActionOutcome {
            status: TagActionStatus::Failed,
            detail: "git push tags failed (exit 1)".into(),
            progress: TagOperationProgress::default(),
        };

        assert_eq!(
            failure
                .clone()
                .with_created_detail("created tag v1.0.0".into()),
            failure
        );
    }
}
