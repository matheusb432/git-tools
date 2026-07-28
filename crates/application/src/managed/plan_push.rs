//! Pure managed-push mode selection.

/// Requests the managed actions implied by a push invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPush {
    /// Supplies the exact commit message, or selects push-only mode when absent.
    pub message: Option<String>,
    /// Selects dry-run semantics for every planned action.
    pub dry: bool,
}

/// Describes the bounded managed actions to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanPushOk {
    /// Push existing commits without first creating managed commits.
    PushOnly { dry: bool },
    /// Commit managed changes, then push only when that commit action is clean.
    CommitThenPush { message: String, dry: bool },
}

/// Selects the managed push mode without performing external actions.
#[cqrsy::query]
pub fn execute(query: PlanPush) -> PlanPushOk {
    let PlanPush { message, dry } = query;
    match message {
        Some(message) => PlanPushOk::CommitThenPush { message, dry },
        None => PlanPushOk::PushOnly { dry },
    }
}

#[cfg(test)]
mod tests {
    use super::{PlanPush, PlanPushOk, execute};

    #[test]
    fn absent_message_plans_push_only() {
        assert_eq!(
            execute(PlanPush {
                message: None,
                dry: true,
            }),
            PlanPushOk::PushOnly { dry: true }
        );
    }

    #[test]
    fn supplied_message_plans_commit_then_push() {
        assert_eq!(
            execute(PlanPush {
                message: Some("save managed work".into()),
                dry: true,
            }),
            PlanPushOk::CommitThenPush {
                message: "save managed work".into(),
                dry: true,
            }
        );
    }
}
