use std::collections::BTreeSet;

use gtl_models::git::{GitRefName, TagName};

/// Reports whether a remote tag push was not started, completed, or became indeterminate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TagRemotePushProgress {
    /// No remote push was attempted.
    #[default]
    NotStarted,
    /// Git confirmed the listed refs were pushed.
    Completed { pushed_refs: BTreeSet<GitRefName> },
    /// Git was started for the listed refs but did not confirm the final remote state.
    Indeterminate {
        attempted_refs: BTreeSet<GitRefName>,
    },
}

/// Reports refs created and published before an outcome or error was reached.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagOperationProgress {
    pub created_refs: BTreeSet<GitRefName>,
    pub remote_push: TagRemotePushProgress,
}

impl TagOperationProgress {
    pub(super) fn created(name: &TagName) -> Self {
        Self {
            created_refs: [GitRefName::for_tag(name)].into_iter().collect(),
            ..Self::default()
        }
    }

    pub(super) fn record_created(&mut self, name: &TagName) {
        self.created_refs.insert(GitRefName::for_tag(name));
    }

    pub(super) fn record_push_attempt(&mut self, names: &[TagName]) {
        self.remote_push = TagRemotePushProgress::Indeterminate {
            attempted_refs: names.iter().map(GitRefName::for_tag).collect(),
        };
    }

    pub(super) fn merge(&mut self, prior: Self) {
        self.created_refs.extend(prior.created_refs);
        if self.remote_push == TagRemotePushProgress::NotStarted {
            self.remote_push = prior.remote_push;
        }
    }
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

/// A locally created tag action with refs that are not yet published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedTagAction {
    detail: String,
    progress: TagOperationProgress,
}

/// A tag action that required no remote mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoopTagAction {
    detail: String,
    progress: TagOperationProgress,
}

/// A tag action whose requested refs were confirmed at the remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushedTagAction {
    detail: String,
    progress: TagOperationProgress,
}

/// A rejected tag action with any effects completed before the failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedTagAction {
    detail: String,
    progress: TagOperationProgress,
}

/// Reports one closed tag action without permitting status/progress mismatches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagActionOutcome {
    /// Local refs were created and no remote push was requested.
    Created(CreatedTagAction),
    /// The requested refs already had the desired remote state.
    Noop(NoopTagAction),
    /// The requested refs were confirmed at the remote.
    Pushed(PushedTagAction),
    /// Validation or Git rejected the operation after the recorded progress.
    Failed(FailedTagAction),
}

impl TagActionOutcome {
    pub(super) fn created(detail: impl Into<String>, name: &TagName) -> Self {
        Self::Created(CreatedTagAction {
            detail: detail.into(),
            progress: TagOperationProgress::created(name),
        })
    }

    pub(super) fn noop(detail: impl Into<String>, created_refs: BTreeSet<GitRefName>) -> Self {
        Self::Noop(NoopTagAction {
            detail: detail.into(),
            progress: TagOperationProgress {
                created_refs,
                remote_push: TagRemotePushProgress::NotStarted,
            },
        })
    }

    pub(super) fn pushed(
        detail: impl Into<String>,
        created_refs: BTreeSet<GitRefName>,
        pushed_refs: BTreeSet<GitRefName>,
    ) -> Self {
        Self::Pushed(PushedTagAction {
            detail: detail.into(),
            progress: TagOperationProgress {
                created_refs,
                remote_push: TagRemotePushProgress::Completed { pushed_refs },
            },
        })
    }

    pub(super) fn failed(detail: impl Into<String>) -> Self {
        Self::failed_with_progress(detail, TagOperationProgress::default())
    }

    pub(super) fn failed_with_progress(
        detail: impl Into<String>,
        progress: TagOperationProgress,
    ) -> Self {
        Self::Failed(FailedTagAction {
            detail: detail.into(),
            progress,
        })
    }

    /// Derives the presentation classification from the closed outcome.
    pub const fn status(&self) -> TagActionStatus {
        match self {
            Self::Created(_) => TagActionStatus::Created,
            Self::Noop(_) => TagActionStatus::Noop,
            Self::Pushed(_) => TagActionStatus::Pushed,
            Self::Failed(_) => TagActionStatus::Failed,
        }
    }

    /// Reports whether the action ended in a closed failure.
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// Returns the exact user-facing workflow detail.
    pub fn detail(&self) -> &str {
        match self {
            Self::Created(outcome) => &outcome.detail,
            Self::Noop(outcome) => &outcome.detail,
            Self::Pushed(outcome) => &outcome.detail,
            Self::Failed(outcome) => &outcome.detail,
        }
    }

    /// Returns effects completed before the outcome was reached.
    pub const fn progress(&self) -> &TagOperationProgress {
        match self {
            Self::Created(outcome) => &outcome.progress,
            Self::Noop(outcome) => &outcome.progress,
            Self::Pushed(outcome) => &outcome.progress,
            Self::Failed(outcome) => &outcome.progress,
        }
    }

    pub(super) fn with_created_ref(mut self, name: &TagName) -> Self {
        if let Self::Created(outcome) = &mut self {
            outcome.detail = format!("{}\ncreated tag {name}", outcome.detail);
            outcome.progress.record_created(name);
        }
        self
    }

    pub(super) fn with_created_detail(self, created_detail: String) -> Self {
        match self {
            Self::Pushed(mut outcome) => {
                outcome.detail = format!("{created_detail}\n{}", outcome.detail);
                Self::Pushed(outcome)
            }
            Self::Noop(mut outcome) => {
                outcome.detail = created_detail;
                Self::Noop(outcome)
            }
            outcome @ (Self::Created(_) | Self::Failed(_)) => outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use gtl_models::git::GitRefName;

    use super::{TagActionOutcome, TagActionStatus, TagOperationProgress};
    use crate::utils::tag_name;

    #[test]
    fn pushed_outcome_appends_to_the_multi_line_creation_detail() {
        assert_eq!(
            TagActionOutcome::pushed(
                "pushed 2 tags: v1.0.0, stable",
                BTreeSet::default(),
                [
                    GitRefName::for_tag(&tag_name("v1.0.0")),
                    GitRefName::for_tag(&tag_name("stable")),
                ]
                .into_iter()
                .collect(),
            )
            .with_created_detail("created tag v1.0.0\ncreated tag stable".into()),
            TagActionOutcome::pushed(
                "created tag v1.0.0\ncreated tag stable\npushed 2 tags: v1.0.0, stable",
                BTreeSet::default(),
                [
                    GitRefName::for_tag(&tag_name("v1.0.0")),
                    GitRefName::for_tag(&tag_name("stable")),
                ]
                .into_iter()
                .collect(),
            )
        );
    }

    #[test]
    fn failed_push_keeps_the_push_failure_detail() {
        let failure = TagActionOutcome::failed("git push tags failed (exit 1)");

        assert_eq!(
            failure
                .clone()
                .with_created_detail("created tag v1.0.0".into()),
            failure
        );
    }

    #[test]
    fn status_and_progress_are_derived_from_the_closed_variant() {
        let created = TagActionOutcome::created("created tag v1.0.0", &tag_name("v1.0.0"));

        assert_eq!(created.status(), TagActionStatus::Created);
        assert_eq!(
            created.progress(),
            &TagOperationProgress::created(&tag_name("v1.0.0"))
        );
    }
}
