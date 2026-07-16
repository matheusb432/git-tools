//! The `discovery` feature slice: find every git repo under a root.
//!
//! [`rules`] holds the pure prune/worktree/skip/label logic (values in, decision
//! out); [`find_repos`] is the query that drives the [`crate::ports::RepoDiscovery`]
//! port and labels the results; [`resolve_repo_top`] resolves one caller path;
//! [`find_repo_tops`] resolves every discovered repo through that operation. The
//! filesystem walk itself lives in the infra adapter, which calls [`rules`]
//! during traversal.

pub mod find_repo_tops;
pub mod find_repos;
pub mod resolve_repo_top;
pub mod rules;
