//! The `discovery` feature slice: find every git repo under a root.
//!
//! [`rules`] holds the pure prune/worktree/skip/label logic (values in, decision
//! out); [`find_repos`] is the query that drives the [`crate::ports::RepoDiscovery`]
//! port and labels the results. The filesystem walk itself lives in the infra
//! adapter, which calls [`rules`] during traversal.

pub mod find_repos;
pub mod rules;
