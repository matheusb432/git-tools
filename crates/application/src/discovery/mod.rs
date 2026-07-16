//! The `discovery` feature slice: find every git repo under a root.
//!
//! [`rules`] holds the pure prune/worktree/skip/label logic (values in, decision
//! out); [`find_repos`] is the query that drives the [`crate::ports::RepoDiscovery`]
//! port and labels the results; [`find_repo_tops`] additionally resolves each
//! repo to its canonical top-level path through the git port. The filesystem walk itself lives in
//! the infra adapter, which calls [`rules`] during traversal.

pub mod find_repo_tops;
pub mod find_repos;
pub mod rules;
