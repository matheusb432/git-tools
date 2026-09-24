//! Prepared, single-use pushes owned by the server rather than a mounted view.

pub mod create_viewer_push;
pub mod execute_viewer_push;
pub mod get_viewer_push;
pub mod get_viewer_push_availability;
mod plan;
pub mod refresh_push_views;
#[cfg(test)]
mod tests;
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

use gtl_models::{
    diffs::CommitId,
    failure::{Classified, ErrorMeta, Failure, PushFailure, Resource},
    git::{BranchName, GitRefName, RemoteName, RemoteUrl},
    paths::RepositoryRoot,
};
use gtl_wire::viewer::push::{ViewerPushId, ViewerPushPreview, ViewerPushStatus};

const OPERATIONS_MAX: u32 = 32;
const REVIEW_LIFETIME: Duration = Duration::from_mins(10);

/// Mutable Git facts needed to check a prepared target before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushRepository {
    pub branch: BranchName,
    pub remote: RemoteName,
    pub destination: GitRefName,
    pub url: RemoteUrl,
    pub head: CommitId,
    pub upstream: CommitId,
}

/// Narrow Git boundary for the viewer's explicit commit push.
pub trait ViewerPushGit: Send + Sync {
    fn inspect_push(&self, path: &RepositoryRoot) -> Result<PushRepository, PushError>;
    fn contains_commit(
        &self,
        path: &RepositoryRoot,
        commit: &CommitId,
        head: &CommitId,
    ) -> Result<bool, PushError>;
    fn count_commits(
        &self,
        path: &RepositoryRoot,
        base: &CommitId,
        head: &CommitId,
    ) -> Result<u64, PushError>;
    fn push_commit(&self, plan: &PushPlan) -> Result<(), PushError>;
}

/// Only application preparation constructs this immutable execution target.
#[derive(Debug, Clone)]
pub struct PushPlan {
    path: RepositoryRoot,
    repository: PushRepository,
    commit: CommitId,
    count: u64,
}

impl PushPlan {
    #[must_use]
    pub fn path(&self) -> &RepositoryRoot {
        &self.path
    }

    /// These exact arguments are shared by the preview and the process adapter.
    #[must_use]
    pub fn arguments(&self) -> Vec<String> {
        vec![
            "-c".into(),
            format!("remote.{}.mirror=false", self.repository.remote),
            "push".into(),
            "--atomic".into(),
            "--porcelain".into(),
            "--no-follow-tags".into(),
            "--recurse-submodules=no".into(),
            "--".into(),
            self.repository.remote.to_string(),
            format!("{}:{}", self.commit, self.repository.destination),
        ]
    }

    fn preview(&self) -> ViewerPushPreview {
        let mut arguments = vec!["git".to_owned(), "-C".into(), self.path.to_string()];
        arguments.extend(self.arguments());
        ViewerPushPreview {
            repository: self.path.clone(),
            destination: format!(
                "{}/{}",
                self.repository.remote,
                self.repository
                    .destination
                    .as_ref()
                    .strip_prefix("refs/heads/")
                    .unwrap_or(self.repository.destination.as_ref())
            ),
            commit: self.commit.clone(),
            count: self.count,
            command: arguments
                .iter()
                .map(|value| shell_argument(value))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

fn shell_argument(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&byte))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum PushError {
    #[error(transparent)]
    #[meta(failure)]
    Refused(#[from] PushFailure),
    #[error("the push operation is no longer available")]
    #[meta(failure = Failure::Gone { resource: Resource::PushOperation })]
    NotFound,
    #[error("the push operation registry lock is poisoned")]
    #[meta(private(Internal))]
    State,
    #[error("the push worker stopped before reporting a result")]
    #[meta(private(Internal))]
    Interrupted,
    #[error(transparent)]
    #[meta(transparent)]
    Source(#[from] super::source::ViewerSourceError),
}

#[derive(Default)]
pub struct ViewerPushOperations(Mutex<PushOperationsState>);

#[derive(Default)]
struct PushOperationsState {
    history: VecDeque<PushOperation>,
    confirmation_order_last: u64,
}

struct PushOperation {
    id: ViewerPushId,
    created: Instant,
    plan: Option<PushPlan>,
    status: ViewerPushStatus,
    confirmation_order: u64,
}

impl PushOperationsState {
    fn repository_running(&self, path: &RepositoryRoot) -> bool {
        self.history.iter().any(|operation| {
            operation.status == ViewerPushStatus::Running
                && operation
                    .plan
                    .as_ref()
                    .is_some_and(|plan| &plan.path == path)
        })
    }
}

impl ViewerPushOperations {
    fn insert(&self, result: Result<PushPlan, PushError>) -> Result<ViewerPushId, PushError> {
        let mut state = self.0.lock().map_err(|_| PushError::State)?;
        if state.history.len() >= OPERATIONS_MAX as usize {
            let index = state
                .history
                .iter()
                .position(|operation| {
                    !matches!(
                        operation.status,
                        ViewerPushStatus::Queued | ViewerPushStatus::Running
                    )
                })
                .ok_or(PushFailure::HistoryFull {
                    operations_max: OPERATIONS_MAX,
                })?;
            state.history.remove(index);
        }
        let id = ViewerPushId::generate();
        let (plan, status) = match result {
            Ok(plan) => {
                let preview = plan.preview();
                (Some(plan), ViewerPushStatus::Review(preview))
            }
            Err(error) => (
                None,
                ViewerPushStatus::Failed {
                    failure: error.classify().into_failure(),
                },
            ),
        };
        state.history.push_back(PushOperation {
            id,
            created: Instant::now(),
            plan,
            status,
            confirmation_order: 0,
        });
        Ok(id)
    }

    /// Atomically confirms a review; duplicate starts never repeat a push.
    pub fn queue(&self, id: ViewerPushId) -> Result<(), PushError> {
        let mut state = self.0.lock().map_err(|_| PushError::State)?;
        let index = state
            .history
            .iter()
            .position(|operation| operation.id == id)
            .ok_or(PushError::NotFound)?;
        if !matches!(state.history[index].status, ViewerPushStatus::Review(_)) {
            return Ok(());
        }
        if state.history[index].created.elapsed() > REVIEW_LIFETIME {
            state.history[index].status = ViewerPushStatus::Failed {
                failure: PushFailure::ReviewExpired.into(),
            };
            return Ok(());
        }
        state.confirmation_order_last = state
            .confirmation_order_last
            .checked_add(1)
            .ok_or(PushError::State)?;
        state.history[index].confirmation_order = state.confirmation_order_last;
        state.history[index].status = ViewerPushStatus::Queued;
        Ok(())
    }

    /// Admits the earliest confirmed push whose repository is idle.
    pub fn begin_next(&self) -> Result<Option<(ViewerPushId, PushPlan)>, PushError> {
        let mut state = self.0.lock().map_err(|_| PushError::State)?;
        let next_id = state
            .history
            .iter()
            .filter(|operation| operation.status == ViewerPushStatus::Queued)
            .filter(|operation| {
                operation
                    .plan
                    .as_ref()
                    .is_some_and(|plan| !state.repository_running(&plan.path))
            })
            .min_by_key(|operation| operation.confirmation_order)
            .map(|operation| operation.id);
        let Some(next_id) = next_id else {
            return Ok(None);
        };
        let operation = state
            .history
            .iter_mut()
            .find(|operation| operation.id == next_id)
            .ok_or(PushError::State)?;
        let plan = operation.plan.clone().ok_or(PushError::State)?;
        operation.status = ViewerPushStatus::Running;
        Ok(Some((next_id, plan)))
    }

    pub fn finish(&self, id: ViewerPushId, result: Result<(), PushError>) -> Result<(), PushError> {
        let mut state = self.0.lock().map_err(|_| PushError::State)?;
        let operation = state
            .history
            .iter_mut()
            .find(|operation| operation.id == id)
            .ok_or(PushError::NotFound)?;
        if operation.status != ViewerPushStatus::Running {
            return Err(PushError::State);
        }
        operation.status = match result {
            Ok(()) => ViewerPushStatus::Succeeded,
            Err(error) => ViewerPushStatus::Failed {
                failure: error.classify().into_failure(),
            },
        };
        Ok(())
    }
}
