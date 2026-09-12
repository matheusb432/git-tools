mod push_pull;
mod push_summary;
mod status;

pub use push_pull::{PushPullResult, RepoSyncStatus, run_pull_all, run_push_all};
pub(crate) use push_summary::{PushOutcome, PushSummary};
pub use status::{StatusResult, run_status, run_status_current, run_status_recursive};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedExit {
    Clean,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedOptions {
    pub dry: bool,
    pub output: ManagedOutput,
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
