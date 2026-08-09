//! The managed-repos feature: commit, prune, push, pull, status, and unpushed
//! selection operations over already-resolved managed projects.

pub mod commit_all;
mod logic;
pub mod plan_push;
pub mod prune_all;
pub mod pull_all;
pub mod push_all;
pub mod select_unpushed;
pub mod status_repos;

pub use logic::service::{RepoSyncResult, SyncExit, SyncStatus};
