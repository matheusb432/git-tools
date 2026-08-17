//! Git repository operations, including branch transitions, local synchronization,
//! recursive traversal, status inspection, and recursive pushes.

pub mod apply_commit;
pub mod apply_prune;
pub mod apply_push;
pub mod apply_rebase;
pub mod apply_recursive_push;
pub mod apply_revert;
pub mod apply_switch;
mod branch_recovery;
pub mod build_recipes;
mod commit_progress;
pub mod find_repositories;
pub mod find_repository_roots;
pub mod get_repository_statuses;
pub mod plan_commit;
pub mod plan_prune;
pub mod plan_push;
pub mod plan_rebase;
pub mod plan_recursive_push;
pub mod plan_revert;
pub mod plan_switch;
pub mod resolve_repository_root;
pub(crate) mod working_tree;

pub use branch_recovery::BranchRecovery;
pub use commit_progress::CommitProgress;
