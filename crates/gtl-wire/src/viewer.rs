//! Typed values exchanged by the desktop viewer and its Dioxus Web shell.

use gtl_models::diffs::CommitId;
use serde::{Deserialize, Serialize};

use crate::recipes::Recipe;

pub const VIEWER_STATE_CHANGED_EVENT: &str = "viewer-state-changed";
pub const VIEWER_DIFF_LINES_PAGE_MAX_BYTES: usize = 256 * 1024;
pub const VIEWER_ARTIFACT_RUNTIME_ID: &str = "gtl-artifact-runtime";
pub const VIEWER_ARTIFACT_MANIFEST_ID: &str = "gtl-artifact-manifest";
pub const VIEWER_ARTIFACT_SYNTAX_ID: &str = "gtl-artifact-syntaxes";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerTheme {
    Dark,
    Light,
    Hearth,
    Mirage,
    Glacier,
    Noir,
    Graphite,
}

impl ViewerTheme {
    /// Returns the stable theme token used by serialized state and document roots.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Hearth => "hearth",
            Self::Mirage => "mirage",
            Self::Glacier => "glacier",
            Self::Noir => "noir",
            Self::Graphite => "graphite",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerDiffLayout {
    Unified,
    Split,
}

impl ViewerDiffLayout {
    /// Returns the stable layout token used by serialized state and document roots.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unified => "unified",
            Self::Split => "split",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerDiffDensity {
    Compact,
    Full,
}

impl ViewerDiffDensity {
    /// Returns the stable density token used by serialized state and document roots.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerRenderOptions {
    pub layout: ViewerDiffLayout,
    pub density: ViewerDiffDensity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerViewIdentity {
    pub tab_id: u64,
    pub range_generation: u64,
    pub selection_generation: u64,
    pub render_options: ViewerRenderOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerTabKind {
    Snapshot,
    Live,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ViewerTabState {
    Pending,
    Ready,
    Broken,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerTab {
    pub id: u64,
    pub label: String,
    pub kind: ViewerTabKind,
    pub state: ViewerTabState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerFileStatus {
    Added,
    Deleted,
    Renamed,
    Modified,
}

/// Opaque address of one file within an identity-bound diff view.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ViewerDiffFileId(String);

impl ViewerDiffFileId {
    /// Creates the stable ID for a file's source-order position.
    pub fn for_index(index: usize) -> Self {
        Self(format!("file-{index}"))
    }

    /// Returns the opaque wire value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerFileSummary {
    pub id: ViewerDiffFileId,
    pub path: String,
    pub absolute_path: String,
    pub anchor_id: String,
    pub added: u32,
    pub removed: u32,
    pub status: ViewerFileStatus,
    pub can_open_in_editor: bool,
    pub initially_expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerCommitSummary {
    pub id: CommitId,
    pub subject: String,
    pub body: String,
    pub date: String,
    pub iso: String,
    pub is_merge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerCommandLine {
    pub lead: String,
    pub range: String,
    pub trail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerFooter {
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerAppliedExclusions {
    pub extensions: Vec<String>,
    pub hidden_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerFailureCode {
    #[serde(rename = "DirNotFound")]
    RepositoryDirectoryNotFound,
    #[serde(rename = "DirNotGitRepo")]
    RepositoryDirectoryNotGitRepository,
    #[serde(rename = "SourceUnavailable")]
    SourceUnavailable,
    #[serde(rename = "RenderFailed")]
    RenderFailed,
}

impl ViewerFailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RepositoryDirectoryNotFound => "DirNotFound",
            Self::RepositoryDirectoryNotGitRepository => "DirNotGitRepo",
            Self::SourceUnavailable => "SourceUnavailable",
            Self::RenderFailed => "RenderFailed",
        }
    }
}

impl std::fmt::Display for ViewerFailureCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ViewerCommitSelection {
    None,
    Pending { id: CommitId },
    Ready { id: CommitId },
    Error { id: CommitId, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerActiveView {
    pub identity: ViewerViewIdentity,
    pub title: String,
    pub repository_name: String,
    pub branch: String,
    pub upstream: String,
    pub command: ViewerCommandLine,
    pub files: Vec<ViewerFileSummary>,
    pub commits_label: String,
    pub commits: Vec<ViewerCommitSummary>,
    pub commit_selection: ViewerCommitSelection,
    pub footer: ViewerFooter,
    pub exclusions: Option<ViewerAppliedExclusions>,
}

// TODO: move this logic to a client context once a context to manage ViewerActiveView state is
// created
/// Makes decision to handle commit selection in UI
pub fn make_commit_selection_action(
    commit_selection: &ViewerCommitSelection,
    tab_id: u64,
    commit_id: CommitId,
) -> CommitSelectionAction {
    match commit_selection {
        ViewerCommitSelection::Ready { id } if id == &commit_id => {
            CommitSelectionAction::UnselectCommit
        }
        ViewerCommitSelection::None
        | ViewerCommitSelection::Error { .. }
        | ViewerCommitSelection::Ready { .. } => {
            CommitSelectionAction::FetchCommit(SelectViewerCommit {
                tab_id,
                id: commit_id,
            })
        }
        ViewerCommitSelection::Pending { .. } => CommitSelectionAction::NoAction,
    }
}

// TODO: move models to feature slice of diff workspace
/// Contextualized action to for UI commit selection
pub enum CommitSelectionAction {
    /// Must unselect commit
    UnselectCommit,
    /// Must fetch given commit with `SelectViewerCommit` request
    FetchCommit(SelectViewerCommit),
    NoAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ViewerActiveState {
    Empty,
    Pending {
        tab_id: u64,
    },
    Broken {
        tab_id: u64,
        code: ViewerFailureCode,
        message: String,
    },
    Error {
        tab_id: u64,
        code: ViewerFailureCode,
        message: String,
    },
    Ready {
        view: Box<ViewerActiveView>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerPreferences {
    pub theme: ViewerTheme,
    pub render_options: ViewerRenderOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewerFeedback {
    TabClosed,
    LiveViewDeleted,
    SnapshotRecipesSkipped { labels: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerShell {
    pub revision: u64,
    pub tabs: Vec<ViewerTab>,
    pub active: ViewerActiveState,
    pub preferences: ViewerPreferences,
    pub feedback: Option<ViewerFeedback>,
}

/// Source-line position within one identity-bound diff file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ViewerDiffCursor(u32);

impl ViewerDiffCursor {
    /// First line in a diff file.
    pub const START: Self = Self(0);

    /// Creates a cursor at a source-line position.
    pub const fn new(position: u32) -> Self {
        Self(position)
    }

    /// Returns the source-line position.
    pub const fn position(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadViewerDiffLines {
    pub identity: ViewerViewIdentity,
    pub file: ViewerDiffFileId,
    pub cursor: ViewerDiffCursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerDiffLines {
    pub identity: ViewerViewIdentity,
    pub file: ViewerDiffFileId,
    pub cursor: ViewerDiffCursor,
    pub lines: Vec<String>,
    pub next: Option<ViewerDiffCursor>,
}

/// Opaque address of one embedded raw-line page in an offline artifact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ViewerArtifactPageId(String);

impl ViewerArtifactPageId {
    /// Derives the page address from every identity-bound request dimension.
    pub fn for_request(request: &LoadViewerDiffLines) -> Self {
        let layout = request.identity.render_options.layout.as_str();
        let density = request.identity.render_options.density.as_str();
        Self(format!(
            "gtl-artifact-page-{}-{}-{}-{layout}-{density}-{}-{}",
            request.identity.tab_id,
            request.identity.range_generation,
            request.identity.selection_generation,
            request.file.as_str(),
            request.cursor.position(),
        ))
    }

    /// Returns the opaque artifact-local address.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One independently decodable raw-line page embedded in an offline artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerArtifactPage {
    pub id: ViewerArtifactPageId,
    pub page: ViewerDiffLines,
}

/// Metadata required to start the client-rendered offline artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerArtifactManifest {
    pub title: String,
    pub theme: ViewerTheme,
    pub views: Vec<ViewerActiveView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cursor", rename_all = "snake_case")]
pub enum ViewerHistoryCursor {
    Newest,
    OlderThan { render_id: i64, page: u32 },
    NewerThan { render_id: i64, page: u32 },
    Oldest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListViewerHistory {
    pub cursor: ViewerHistoryCursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerRecipeKind {
    Diff,
    MergeDiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ViewerHistoryCopyKind {
    Diff,
    MergeDiff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerHistoryEntry {
    pub id: i64,
    pub title: String,
    pub repository_name: String,
    pub kind: ViewerRecipeKind,
    pub range_label: String,
    pub rendered_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerHistoryCopyPayload {
    pub id: i64,
    pub title: String,
    pub repo_name: String,
    pub kind: ViewerHistoryCopyKind,
    pub range_label: String,
    pub rendered_at: String,
    pub recipe: Recipe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerHistoryPage {
    pub entries: Vec<ViewerHistoryEntry>,
    pub total_count: u64,
    pub page_number: u32,
    pub page_count: u32,
    pub has_newer: bool,
    pub has_older: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerProjectDiffExclusions {
    // TODO: refactor to newtype.
    pub project_name: String,
    // TODO: refactor to newtype here, in gtl-models and in gtl-web usages.
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerDiffExclusions {
    pub default_extensions: Vec<String>,
    pub projects: Vec<ViewerProjectDiffExclusions>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerUserSettings {
    pub configuration_path: Option<String>,
    pub configured_theme: Option<ViewerTheme>,
    pub effective_theme: ViewerTheme,
    pub render_options: ViewerRenderOptions,
    pub push_confirmation_required: bool,
    pub diff_exclusions: ViewerDiffExclusions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerTabRequest {
    pub tab_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectViewerCommit {
    pub tab_id: u64,
    pub id: CommitId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenViewerHistory {
    pub render_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetViewerHistoryCopy {
    pub render_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenViewerDiffFile {
    pub identity: ViewerViewIdentity,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "preference", content = "value", rename_all = "snake_case")]
pub enum SetViewerPreference {
    Layout(ViewerDiffLayout),
    Density(ViewerDiffDensity),
    Theme(ViewerTheme),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerResource {
    Shell,
    Tab,
    LiveView,
    Commit,
    DiffLines,
    HistoryEntry,
    Settings,
    DiffFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewerApiError {
    InvalidRequest,
    NotFound { resource: ViewerResource },
    Conflict,
    Unavailable { resource: ViewerResource },
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerStateChanged {
    pub revision: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> LoadViewerDiffLines {
        LoadViewerDiffLines {
            identity: ViewerViewIdentity {
                tab_id: 3,
                range_generation: 5,
                selection_generation: 7,
                render_options: ViewerRenderOptions {
                    layout: ViewerDiffLayout::Unified,
                    density: ViewerDiffDensity::Compact,
                },
            },
            file: ViewerDiffFileId::for_index(11),
            cursor: ViewerDiffCursor::new(13),
        }
    }

    #[test]
    fn artifact_page_ids_bind_every_request_identity_dimension() {
        let request = request();
        let expected = ViewerArtifactPageId::for_request(&request);
        let mut changed_cursor = request.clone();
        changed_cursor.cursor = ViewerDiffCursor::new(14);
        let mut changed_file = request.clone();
        changed_file.file = ViewerDiffFileId::for_index(12);
        let mut changed_layout = request.clone();
        changed_layout.identity.render_options.layout = ViewerDiffLayout::Split;

        assert_ne!(expected, ViewerArtifactPageId::for_request(&changed_cursor));
        assert_ne!(expected, ViewerArtifactPageId::for_request(&changed_file));
        assert_ne!(expected, ViewerArtifactPageId::for_request(&changed_layout));
    }
}
