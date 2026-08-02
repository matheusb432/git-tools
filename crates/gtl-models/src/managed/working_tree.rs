//! Data shapes for a repo's working-tree state, shared by the `commit` and
//! `status` fan-outs. Dumb carriers — the `managed::working_tree` application
//! reader builds them.

use serde::Serialize;

/// One changed working-tree file: its two-letter porcelain status and path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CommitFile {
    pub status: String,
    pub path: String,
}

/// A repo's working-tree state: whether it exists here, whether it is dirty, and
/// the changed files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyState {
    pub present: bool,
    pub dirty: bool,
    pub files: Vec<CommitFile>,
}
