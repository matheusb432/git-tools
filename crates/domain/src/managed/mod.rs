//! The managed-repos feature's domain model: one entry from the fleet manifest,
//! plus the recursive-push, status, and working-tree data shapes.

use std::path::PathBuf;

pub mod push_subrepos;
pub mod status;
pub mod working_tree;

/// One repo listed in the managed-repos manifest (`repos.toml`), resolved to an
/// absolute local path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRepo {
    pub name: String,
    pub path: PathBuf,
    pub remote: String,
}
