//! Plans and advances the managed push workflow without performing its external actions.

/// Requests a managed push workflow.
///
/// # Examples
///
/// ```
/// use application::managed::push_workflow::ManagedPush;
///
/// let request = ManagedPush {
///     message: Some("save work".into()),
///     dry: false,
/// };
/// assert_eq!(request.message.as_deref(), Some("save work"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPush {
    /// Supplies the exact commit message, or starts a push-only workflow when absent.
    pub message: Option<String>,
    /// Selects dry-run semantics for both managed actions.
    pub dry: bool,
}

/// Reports whether an interpreted managed action completed cleanly.
///
/// # Examples
///
/// ```
/// use application::managed::push_workflow::ManagedPushStepStatus;
///
/// assert_ne!(
///     ManagedPushStepStatus::Clean,
///     ManagedPushStepStatus::NonClean,
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedPushStepStatus {
    /// The action completed with the clean managed exit classification.
    Clean,
    /// The action completed with a warning, failure, or usage classification.
    NonClean,
}

/// Records the status of each action reached by a managed push workflow.
///
/// # Examples
///
/// ```
/// use application::managed::push_workflow::ManagedPushWorkflowProgress;
///
/// assert_eq!(ManagedPushWorkflowProgress::default().commit, None);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ManagedPushWorkflowProgress {
    /// Records the commit status when the workflow requested a commit.
    pub commit: Option<ManagedPushStepStatus>,
    /// Records the push status when the workflow reached the push action.
    pub push: Option<ManagedPushStepStatus>,
}

/// Directs the caller to run the managed commit action.
///
/// # Examples
///
/// ```
/// use application::managed::push_workflow::{ManagedPush, ManagedPushDirective, execute};
///
/// let ManagedPushDirective::Commit(commit) = execute(ManagedPush {
///     message: Some("save work".into()),
///     dry: false,
/// }) else {
///     panic!("expected commit");
/// };
/// assert_eq!(commit.message, "save work");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPushCommitDirective {
    /// Carries the exact message supplied to the workflow.
    pub message: String,
    /// Carries the dry-run mode for the managed commit action.
    pub dry: bool,
    progress: ManagedPushWorkflowProgress,
}

impl ManagedPushCommitDirective {
    /// Returns the aggregate workflow progress before this action runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::managed::push_workflow::{
    ///     ManagedPush, ManagedPushDirective, ManagedPushWorkflowProgress, execute,
    /// };
    ///
    /// let ManagedPushDirective::Commit(commit) = execute(ManagedPush {
    ///     message: Some("save work".into()),
    ///     dry: false,
    /// }) else {
    ///     panic!("expected commit");
    /// };
    /// assert_eq!(commit.progress(), &ManagedPushWorkflowProgress::default());
    /// ```
    #[must_use]
    pub const fn progress(&self) -> &ManagedPushWorkflowProgress {
        &self.progress
    }

    /// Advances the workflow with the interpreted commit status.
    ///
    /// A clean commit is the only transition that unlocks the push action.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::managed::push_workflow::{
    ///     ManagedPush, ManagedPushDirective, ManagedPushStepStatus, execute,
    /// };
    ///
    /// let ManagedPushDirective::Commit(commit) = execute(ManagedPush {
    ///     message: Some("save work".into()),
    ///     dry: false,
    /// }) else {
    ///     panic!("expected commit");
    /// };
    /// assert!(matches!(
    ///     commit.complete(ManagedPushStepStatus::Clean),
    ///     ManagedPushDirective::Push(_),
    /// ));
    /// ```
    #[must_use]
    pub fn complete(mut self, status: ManagedPushStepStatus) -> ManagedPushDirective {
        self.progress.commit = Some(status);
        match status {
            ManagedPushStepStatus::Clean => {
                ManagedPushDirective::Push(ManagedPushRemoteDirective {
                    dry: self.dry,
                    progress: self.progress,
                })
            }
            ManagedPushStepStatus::NonClean => ManagedPushDirective::Complete(
                ManagedPushOutcome::StoppedAfterCommit(self.progress),
            ),
        }
    }
}

/// Directs the caller to run the managed push action.
///
/// # Examples
///
/// ```
/// use application::managed::push_workflow::{ManagedPush, ManagedPushDirective, execute};
///
/// assert!(matches!(
///     execute(ManagedPush {
///         message: None,
///         dry: false
///     }),
///     ManagedPushDirective::Push(_),
/// ));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPushRemoteDirective {
    /// Carries the dry-run mode for the managed push action.
    pub dry: bool,
    progress: ManagedPushWorkflowProgress,
}

impl ManagedPushRemoteDirective {
    /// Returns the aggregate workflow progress before this action runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::managed::push_workflow::{
    ///     ManagedPush, ManagedPushDirective, ManagedPushWorkflowProgress, execute,
    /// };
    ///
    /// let ManagedPushDirective::Push(push) = execute(ManagedPush {
    ///     message: None,
    ///     dry: false,
    /// }) else {
    ///     panic!("expected push");
    /// };
    /// assert_eq!(push.progress(), &ManagedPushWorkflowProgress::default());
    /// ```
    #[must_use]
    pub const fn progress(&self) -> &ManagedPushWorkflowProgress {
        &self.progress
    }

    /// Completes the workflow with the interpreted push status.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::managed::push_workflow::{
    ///     ManagedPush, ManagedPushDirective, ManagedPushStepStatus, execute,
    /// };
    ///
    /// let ManagedPushDirective::Push(push) = execute(ManagedPush {
    ///     message: None,
    ///     dry: false,
    /// }) else {
    ///     panic!("expected push");
    /// };
    /// assert!(matches!(
    ///     push.complete(ManagedPushStepStatus::Clean),
    ///     ManagedPushDirective::Complete(_),
    /// ));
    /// ```
    #[must_use]
    pub fn complete(mut self, status: ManagedPushStepStatus) -> ManagedPushDirective {
        self.progress.push = Some(status);
        ManagedPushDirective::Complete(ManagedPushOutcome::Completed(self.progress))
    }
}

/// Describes the next managed action or the completed workflow outcome.
///
/// # Examples
///
/// ```
/// use application::managed::push_workflow::{ManagedPush, ManagedPushDirective, execute};
///
/// assert!(matches!(
///     execute(ManagedPush {
///         message: None,
///         dry: false
///     }),
///     ManagedPushDirective::Push(_),
/// ));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedPushDirective {
    /// Requests the commit action before any push can run.
    Commit(ManagedPushCommitDirective),
    /// Requests the push action.
    Push(ManagedPushRemoteDirective),
    /// Reports the terminal workflow outcome.
    Complete(ManagedPushOutcome),
}

/// Reports why the managed push workflow completed and preserves aggregate progress.
///
/// # Examples
///
/// ```
/// use application::managed::push_workflow::{ManagedPushOutcome, ManagedPushWorkflowProgress};
///
/// let outcome = ManagedPushOutcome::Completed(ManagedPushWorkflowProgress::default());
/// assert!(matches!(outcome, ManagedPushOutcome::Completed(_)));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedPushOutcome {
    /// The workflow reached and interpreted its push action.
    Completed(ManagedPushWorkflowProgress),
    /// A non-clean commit prevented the push action from starting.
    StoppedAfterCommit(ManagedPushWorkflowProgress),
}

/// Starts the managed push workflow and returns its first directive.
///
/// # Examples
///
/// ```
/// use application::managed::push_workflow::{ManagedPush, ManagedPushDirective, execute};
///
/// assert!(matches!(
///     execute(ManagedPush {
///         message: None,
///         dry: false
///     }),
///     ManagedPushDirective::Push(_),
/// ));
/// ```
#[cqrsy::handler(command)]
pub fn execute(command: ManagedPush) -> ManagedPushDirective {
    let ManagedPush { message, dry } = command;
    let progress = ManagedPushWorkflowProgress::default();
    match message {
        Some(message) => ManagedPushDirective::Commit(ManagedPushCommitDirective {
            message,
            dry,
            progress,
        }),
        None => ManagedPushDirective::Push(ManagedPushRemoteDirective { dry, progress }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ManagedPush, ManagedPushDirective, ManagedPushOutcome, ManagedPushStepStatus,
        ManagedPushWorkflowProgress, execute,
    };

    fn request(message: Option<&str>) -> ManagedPush {
        ManagedPush {
            message: message.map(str::to_owned),
            dry: true,
        }
    }

    #[test]
    fn push_without_a_message_starts_with_push_only() {
        let workflow = execute(request(None));

        let ManagedPushDirective::Push(push) = workflow else {
            panic!("push-only workflow must start with push");
        };
        assert!(push.dry);
        assert_eq!(push.progress(), &ManagedPushWorkflowProgress::default());
    }

    #[test]
    fn push_with_a_message_starts_with_the_exact_commit() {
        let workflow = execute(request(Some("save managed work")));

        let ManagedPushDirective::Commit(commit) = workflow else {
            panic!("commit-then-push workflow must start with commit");
        };
        assert_eq!(commit.message, "save managed work");
        assert!(commit.dry);
        assert_eq!(commit.progress(), &ManagedPushWorkflowProgress::default());
    }

    #[test]
    fn a_clean_commit_unlocks_push_and_records_progress() {
        let ManagedPushDirective::Commit(commit) = execute(request(Some("save"))) else {
            panic!("workflow must request commit");
        };

        let ManagedPushDirective::Push(push) = commit.complete(ManagedPushStepStatus::Clean) else {
            panic!("clean commit must unlock push");
        };

        assert_eq!(
            push.progress(),
            &ManagedPushWorkflowProgress {
                commit: Some(ManagedPushStepStatus::Clean),
                push: None,
            }
        );
    }

    #[test]
    fn a_non_clean_commit_stops_before_push_and_records_the_outcome() {
        let ManagedPushDirective::Commit(commit) = execute(request(Some("save"))) else {
            panic!("workflow must request commit");
        };

        let ManagedPushDirective::Complete(outcome) =
            commit.complete(ManagedPushStepStatus::NonClean)
        else {
            panic!("non-clean commit must complete without push");
        };

        assert_eq!(
            outcome,
            ManagedPushOutcome::StoppedAfterCommit(ManagedPushWorkflowProgress {
                commit: Some(ManagedPushStepStatus::NonClean),
                push: None,
            })
        );
    }

    #[test]
    fn push_completion_returns_aggregate_progress_for_both_modes() {
        let ManagedPushDirective::Push(push_only) = execute(request(None)) else {
            panic!("push-only workflow must request push");
        };
        assert_eq!(
            push_only.complete(ManagedPushStepStatus::NonClean),
            ManagedPushDirective::Complete(ManagedPushOutcome::Completed(
                ManagedPushWorkflowProgress {
                    commit: None,
                    push: Some(ManagedPushStepStatus::NonClean),
                }
            ))
        );

        let ManagedPushDirective::Commit(commit) = execute(request(Some("save"))) else {
            panic!("workflow must request commit");
        };
        let ManagedPushDirective::Push(push) = commit.complete(ManagedPushStepStatus::Clean) else {
            panic!("clean commit must unlock push");
        };
        assert_eq!(
            push.complete(ManagedPushStepStatus::Clean),
            ManagedPushDirective::Complete(ManagedPushOutcome::Completed(
                ManagedPushWorkflowProgress {
                    commit: Some(ManagedPushStepStatus::Clean),
                    push: Some(ManagedPushStepStatus::Clean),
                }
            ))
        );
    }
}
