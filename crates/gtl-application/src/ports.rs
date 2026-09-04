//! Port traits: the seams the application core talks through, implemented by
//! `infra` adapters at the composition root. Every port is cheap to clone,
//! thread-safe, and `'static` so process roots can move adapters across worker
//! boundaries.

mod artifact_store;
mod clock;
mod diff_viewer_client;
mod file_system_client;
mod git_client;
mod html_renderer;
mod project_client;
mod text_editor_client;
mod user_settings;

pub use artifact_store::{
    ArtifactMeta, ArtifactRangeKey, ArtifactStore, HistoryRecord, PlacedArtifact,
};
pub use clock::Clock;
pub use diff_viewer_client::{
    DiffRenderOutcome, DiffRenderRequest, DiffRenderResponse, DiffViewerClient,
};
pub use file_system_client::{
    FileSystemClient, FileSystemClientError, FileSystemClientErrorKind, FileSystemEntryKind,
};
pub use git_client::{
    GitClient, GitCommitReceipt, GitDiffFormat, GitDiffRequest, GitEffect, GitPushReceipt,
    GitRepositoryState, GitStatusSnapshot, GitStatusUpstream, GitWorkingTree,
    GitWorkingTreeSummary,
};
pub use html_renderer::HtmlRenderer;
pub use project_client::{
    ProjectCatalogueConfigurationError, ProjectCatalogueDataError,
    ProjectCatalogueUnavailableError, ProjectClient, ProjectClientError,
};
pub use text_editor_client::TextEditorClient;
pub use user_settings::{
    UserSettingsConfigurationError, UserSettingsEditConflict, UserSettingsEditError,
    UserSettingsEditOutcome, UserSettingsEditor, UserSettingsLoadError, UserSettingsReader,
};
