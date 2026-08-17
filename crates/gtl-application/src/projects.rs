//! Git operations whose repository scope is the active sample_project project catalog.

pub mod build_recipes;
pub mod commit_repositories;
pub mod plan_push;
pub mod prune_branches;
pub mod pull_repositories;
pub mod push_repositories;
mod remote_sync;
pub mod render_project_diff;
pub mod select_unpushed_repositories;

pub use remote_sync::{RepoSyncResult, SyncExit, SyncStatus};
