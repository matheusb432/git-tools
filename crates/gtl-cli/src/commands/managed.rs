//! Fanning git operations out across every repo listed in the `repos.toml` manifest
//! (sample_project's managed-repos list): `push --all`, `pull --all`, `commit --all`, `status --all`,
//! and `prune --all`. Each concern lives in its own submodule; this facade owns the shared
//! request/response seam (`ManagedRepo`, `ManagedOptions`, `ManagedRun`, `ManagedExit`) and
//! re-exports each submodule's entry points under the historical `managed::` path.

use std::path::PathBuf;

mod commit;
mod manifest;
mod prune_all;
mod push_pull;
mod push_summary;
mod status;

pub use commit::{CommitFile, CommitResult, run_commit_all};
/// The manifest entry shape — the models type, shared with the application slices.
pub use gtl_models::managed::ManagedRepo;
pub use manifest::{sample_project_manifest_path, load_repos};
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent CLI flags mirrored from argv, not a disguised state machine"
)]
pub struct ManagedOptions {
    pub repos_file: Option<PathBuf>,
    pub home_dir: Option<PathBuf>,
    pub dry: bool,
    pub json: bool,
    pub color: bool,
    pub message_for_all: Option<String>,
    pub interactive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRun<T> {
    pub exit: ManagedExit,
    pub results: Vec<T>,
    pub stdout: String,
    pub stderr: String,
}
