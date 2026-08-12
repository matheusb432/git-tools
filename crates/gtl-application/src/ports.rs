//! Port traits: the seams the application core talks through, implemented by
//! `infra` adapters at the composition root. Every port is cheap to clone,
//! thread-safe, and `'static` so process roots can move adapters across worker
//! boundaries.

mod artifact_store;
mod clock;
mod configured_editor_client;
mod diff_viewer_client;
mod file_system_client;
mod git_client;
mod html_renderer;
mod project_client;
mod push_ledger;
mod repo_discovery;
mod user_settings_store;

pub use artifact_store::{
    ArtifactMeta, ArtifactRangeKey, ArtifactStore, HistoryRecord, PlacedArtifact,
};
pub use clock::Clock;
pub use configured_editor_client::ConfiguredEditorClient;
pub use diff_viewer_client::{
    DiffRenderOutcome, DiffRenderRequest, DiffRenderResponse, DiffViewerClient,
};
pub use file_system_client::{
    FileSystemClient, FileSystemClientError, FileSystemClientErrorKind, FileSystemEntryKind,
};
pub use git_client::{
    GitClient, GitCommitReceipt, GitDiffFormat, GitDiffRequest, GitEffect, GitPushReceipt,
    GitRepositoryState, GitWorkingTree, MergedBranch,
};
pub use html_renderer::HtmlRenderer;
pub use project_client::{ProjectClient, ProjectClientError};
pub use push_ledger::{LedgerEntry, PushLedger};
pub use repo_discovery::RepoDiscovery;
pub use user_settings_store::{UserSettingsEditError, UserSettingsLoadError, UserSettingsStore};
