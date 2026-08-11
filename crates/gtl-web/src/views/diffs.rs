mod client_diff_document;
#[cfg(feature = "desktop")]
mod diff_history;
mod diff_rows;
mod diff_workspace;

pub(crate) use client_diff_document::ClientDiffDocument;
#[cfg(feature = "desktop")]
pub(crate) use diff_history::DiffHistoryView;
pub(crate) use diff_rows::{SplitDiffRowBatch, UnifiedDiffRowBatch};
#[cfg(feature = "artifact")]
pub(crate) use diff_workspace::ArtifactDiffWorkspace;
#[cfg(feature = "desktop")]
pub(crate) use diff_workspace::DiffWorkspaceView;
