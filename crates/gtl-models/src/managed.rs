//! The managed-repos feature's models model: one entry from sample_project's active project catalogue,
//! plus the recursive-push, status, and working-tree data shapes.

use std::path::PathBuf;

pub mod push_subrepos;
pub mod status;
pub mod working_tree;

/// One active sample_project project resolved to an absolute local Git repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRepo {
    pub name: String,
    pub path: PathBuf,
    pub remote: String,
}
