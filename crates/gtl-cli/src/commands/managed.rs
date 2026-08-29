//! Fanning Git operations out across active projects listed by sample_project: `push --all`,
//! `pull --all`, `commit --all`, and `status --all`. Managed push omits
//! its configured project exclusions. Each concern lives in its own submodule;
//! this facade owns the shared request/response seam (`ManagedOptions`, `ManagedRun`,
//! `ManagedExit`) and re-exports each submodule's entry points under the historical
//! `managed::` path.

mod commit;
mod push_pull;
mod push_summary;
mod status;

pub use commit::{CommitFile, CommitResult, run_commit_all, run_commit_for_push_all};
pub use push_pull::{PushPullResult, RepoSyncStatus, run_pull_all, run_push_all};
pub(crate) use push_summary::{PushOutcome, PushSummary};
pub use status::{StatusResult, run_status, run_status_current, run_status_recursive};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedExit {
    Clean,
    Warn,
    Fail,
    Usage,
}

impl ManagedExit {
    #[must_use]
    pub fn code(self) -> i32 {
        match self {
            ManagedExit::Clean => 0,
            ManagedExit::Warn => 1,
            ManagedExit::Fail | ManagedExit::Usage => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedOptions {
    pub dry: bool,
    pub output: ManagedOutput,
    pub message_for_all: Option<String>,
    pub interactive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedOutput {
    Text { color: bool },
    Json,
}

impl ManagedOutput {
    #[must_use]
    pub const fn from_flags(json: bool, color: bool) -> Self {
        if json {
            Self::Json
        } else {
            Self::Text { color }
        }
    }

    #[must_use]
    pub const fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }

    #[must_use]
    pub const fn color_enabled(self) -> bool {
        matches!(self, Self::Text { color: true })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRun<T> {
    pub exit: ManagedExit,
    pub results: Vec<T>,
    pub stdout: String,
    pub stderr: String,
}
