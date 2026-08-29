//! Pure project-push mode selection.

use gtl_models::git::GitEffectMode;

/// Requests the project actions implied by a push invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPush {
    /// Supplies the exact commit message, or selects push-only mode when absent.
    pub message: Option<String>,
    /// Selects dry-run semantics for every planned action.
    pub mode: GitEffectMode,
}

/// Describes the bounded project actions to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanPushOk {
    /// Push existing commits without first creating project commits.
    PushOnly { mode: GitEffectMode },
    /// Commit project changes, then push only when that commit action is clean.
    CommitThenPush {
        message: String,
        mode: GitEffectMode,
    },
}

/// Selects the project push mode without performing external actions.
#[cqrsy::query]
#[must_use]
pub fn execute(query: PlanPush) -> PlanPushOk {
    let PlanPush { message, mode } = query;
    match message {
        Some(message) => PlanPushOk::CommitThenPush { message, mode },
        None => PlanPushOk::PushOnly { mode },
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::git::GitEffectMode;

    use super::{PlanPush, PlanPushOk};
    use crate::projects::plan_push;

    #[test]
    fn absent_message_plans_push_only() {
        assert_eq!(
            plan_push::execute(PlanPush {
                message: None,
                mode: GitEffectMode::DryRun,
            }),
            PlanPushOk::PushOnly {
                mode: GitEffectMode::DryRun
            }
        );
    }

    #[test]
    fn supplied_message_plans_commit_then_push() {
        assert_eq!(
            plan_push::execute(PlanPush {
                message: Some("save project work".into()),
                mode: GitEffectMode::DryRun,
            }),
            PlanPushOk::CommitThenPush {
                message: "save project work".into(),
                mode: GitEffectMode::DryRun,
            }
        );
    }
}
