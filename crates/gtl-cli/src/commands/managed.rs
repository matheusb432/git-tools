//! Fanning git operations out across every active project listed by sample_project: `push --all`,
//! `pull --all`, `commit --all`, `status --all`,
//! and `prune --all`. Each concern lives in its own submodule; this facade owns the shared
//! request/response seam (`ProjectRepository`, `ManagedOptions`, `ManagedRun`, `ManagedExit`) and
//! re-exports each submodule's entry points under the historical `managed::` path.

mod commit;
mod project_catalog;
mod prune_all;
mod push_pull;
mod push_summary;
mod status;

pub use commit::{CommitFile, CommitResult, run_commit_all};
/// The managed project shape shared with the application slices.
pub use gtl_models::projects::ProjectRepository;
pub use project_catalog::load_projects;
pub use prune_all::{PruneRepoResult, PrunedBranch, run_prune_all};
pub use push_pull::{PushPullResult, run_pull_all, run_push_all};
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
    pub const fn from_flags(json: bool, color: bool) -> Self {
        if json {
            Self::Json
        } else {
            Self::Text { color }
        }
    }

    pub const fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }

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
