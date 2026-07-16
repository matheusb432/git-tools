//! The managed-repos feature: commit, prune, push, pull, status, and unpushed
//! selection operations over already-resolved manifest entries.

pub mod commit_all;
pub mod prune_all;
pub mod pull_all;
pub mod push_all;
pub mod push_workflow;
pub mod select_unpushed;
pub mod service;
pub mod status_repos;
pub mod working_tree;
