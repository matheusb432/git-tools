//! Port traits: the seams the application core talks through, implemented by
//! `infra` adapters at the composition root. Every port is cheap to clone,
//! thread-safe, and `'static` so process roots can move adapters across worker
//! boundaries.

mod app_state_store;
mod artifact_store;
mod clock;
mod configured_editor_client;
mod diff_source;
mod file_system_client;
mod git_runner;
mod html_renderer;
mod managed_manifest;
mod push_ledger;
mod remote_sync;
mod repo_discovery;
mod repo_probe;
mod user_settings_editor;
mod user_settings_store;

pub use app_state_store::AppStateStore;
pub use artifact_store::{ArtifactMeta, ArtifactStore, HistoryRecord, PlacedArtifact};
pub use clock::Clock;
pub use configured_editor_client::ConfiguredEditorClient;
pub use diff_source::DiffSource;
pub use file_system_client::{
    FileSystemClient, FileSystemClientError, FileSystemClientErrorKind, FileSystemEntryKind,
};
pub use git_runner::{GitOutput, GitRunner};
pub use html_renderer::HtmlRenderer;
pub use managed_manifest::ManagedManifest;
pub use push_ledger::{LedgerEntry, PushLedger};
pub use remote_sync::{RemoteSync, SyncOutput};
pub use repo_discovery::RepoDiscovery;
pub use repo_probe::{RepoProbe, RepoProbeResult};
pub use user_settings_editor::{UserSettingsEditError, UserSettingsEditor};
pub use user_settings_store::{AppSettings, UserSettingsStore};
