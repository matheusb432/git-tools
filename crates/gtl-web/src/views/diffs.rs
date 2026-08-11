mod client_diff_document;
#[cfg(feature = "desktop")]
mod diff_history;
mod diff_rows;
mod diff_workspace;
mod file_status_badge;
mod line_changes;

pub(crate) use client_diff_document::ClientDiffDocument;
#[cfg(feature = "desktop")]
pub(crate) use diff_history::DiffHistoryView;
pub(crate) use diff_rows::{SplitDiffRowBatch, UnifiedDiffRowBatch};
#[cfg(feature = "artifact")]
pub(crate) use diff_workspace::ArtifactDiffWorkspace;
#[cfg(feature = "desktop")]
pub(crate) use diff_workspace::DiffWorkspaceView;
pub(crate) use file_status_badge::{DiffFileStatusBadge, DiffFileStatusBadgeSize};
pub(crate) use line_changes::{DiffLineChangeBadge, DiffLineChangeKind, DiffLineChangeText};
