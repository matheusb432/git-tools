mod client_diff_document;
#[cfg(feature = "desktop")]
mod diff_history;
mod diff_rows;
pub(crate) mod diff_workspace;
mod file_status;
mod line_changes;
#[cfg(feature = "desktop")]
pub(crate) mod project_diff;
mod search_keybindings;

#[cfg(feature = "desktop")]
pub(crate) use client_diff_document::ClientDiffDocument;
#[cfg(feature = "artifact")]
pub(crate) use client_diff_document::StaticDiffDocument;
#[cfg(feature = "desktop")]
pub(crate) use diff_history::DiffHistoryView;
pub(crate) use diff_rows::{SplitDiffRowBatch, UnifiedDiffRowBatch};
#[cfg(feature = "artifact")]
pub(crate) use diff_workspace::ArtifactDiffWorkspace;
#[cfg(feature = "desktop")]
pub(crate) use diff_workspace::DiffWorkspaceView;
pub(crate) use file_status::{DiffFileStatus, file_status_text_class};
pub(crate) use line_changes::{DiffLineChangeBadge, DiffLineChangeKind, DiffLineChangeText};
#[cfg(feature = "desktop")]
pub(crate) use project_diff::ProjectDiffView;
