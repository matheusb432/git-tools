//! Fanning git operations out across every repo listed in the `repos.toml` manifest
//! (sample_project's managed-repos list): `push-all`, `pull-all`, `commit-all`, `status`,
//! and `prune --all`. Each concern lives in its own submodule; this facade owns the shared
//! request/response seam (`ManagedRepo`, `ManagedOptions`, `ManagedRun`, `ManagedExit`) and
//! re-exports each submodule's entry points under the historical `managed::` path.

use std::path::PathBuf;

mod commit;
mod git_capture;
mod manifest;
mod prune_all;
mod push_pull;
mod status;
#[cfg(test)]
mod test_support;
mod working_tree;

pub use commit::{CommitFile, CommitResult, run_commit_all};
pub use manifest::{sample_project_manifest_path, load_repos};
pub use prune_all::{PruneRepoResult, PrunedBranch, run_prune_all};
pub use push_pull::{PushPullResult, run_pull_all, run_push_all};
pub use status::{StatusResult, run_status, run_status_current, run_status_recursive};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRepo {
    pub name: String,
    pub path: PathBuf,
    pub remote: String,
}

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
            ManagedExit::Fail => 2,
            ManagedExit::Usage => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
