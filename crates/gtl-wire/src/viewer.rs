//! Typed values exchanged by the desktop viewer and its Dioxus Web shell.

use gtl_models::{
    diffs::{CommitId, DiffLineCount, ExcludedExtensions},
    git::{GitHead, GitRevision},
    paths::{AbsoluteFilePath, ProjectName, RepositoryRelativePath},
    timestamps::MachineTimestamp,
    viewer::{
        HistoryPageNumber, HistoryPagePosition, HistoryRenderCount, RenderHistoryId,
        ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId, ViewerVersion,
    },
};
use nutype::nutype;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

pub const VIEWER_PROTOCOL_VERSION: u32 = 2;
pub const VIEWER_COMMIT_PAGE_MAX_ENTRIES: usize = 100;
pub const VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES: usize = 256 * 1024;
pub const VIEWER_COMMIT_BODY_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const VIEWER_ROW_BATCH_MAX_ROWS: usize = 64;
pub const VIEWER_ROW_BATCH_MAX_ENCODED_BYTES: usize = 256 * 1024;
pub const VIEWER_ROW_MAX_ENCODED_BYTES: usize = 4 * 1024 * 1024;

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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    pub tab_id: ViewerTabId,
    pub range_generation: ViewerRangeGeneration,
    pub selection_generation: ViewerSelectionGeneration,
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
    pub id: ViewerTabId,
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
pub struct ViewerDiffFileId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewerDiffFileIdError;

impl std::fmt::Display for ViewerDiffFileIdError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("viewer diff file ID must not be empty")
    }
}

impl std::error::Error for ViewerDiffFileIdError {}

impl AsRef<str> for ViewerDiffFileId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl ViewerDiffFileId {
    /// Creates the stable ID for a file's source-order position.
    #[must_use]
    pub fn for_index(index: usize) -> Self {
        Self(format!("file-{index}"))
    }

    /// Returns the opaque wire value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

impl TryFrom<String> for ViewerDiffFileId {
    type Error = ViewerDiffFileIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Err(ViewerDiffFileIdError)
        } else {
            Ok(Self(value))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerFileSummary {
    pub id: ViewerDiffFileId,
    pub path: RepositoryRelativePath,
    pub absolute_path: AbsoluteFilePath,
    pub anchor_id: String,
    pub added: DiffLineCount,
    pub removed: DiffLineCount,
    pub status: ViewerFileStatus,
    pub can_open_in_editor: bool,
    pub initially_expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerCommitSummary {
    pub id: CommitId,
    pub subject: String,
    pub body: String,
    pub committed_at: MachineTimestamp,
    pub is_merge: bool,
}

/// Position of the next commit in one identity-bound commit list.
#[nutype(
    const_fn,
    default = 0,
    derive(
        Debug,
        Clone,
        Copy,
        Default,
        PartialEq,
        Eq,
        Hash,
        Serialize,
        Deserialize
    )
)]
pub struct ViewerCommitCursor(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListViewerCommits {
    pub identity: ViewerViewIdentity,
    pub cursor: Option<ViewerCommitCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerCommitPage {
    pub identity: ViewerViewIdentity,
    pub commits: Vec<ViewerCommitSummary>,
    pub next_cursor: Option<ViewerCommitCursor>,
}

#[derive(Serialize)]
struct ViewerCommitSummaryRef<'a> {
    id: &'a CommitId,
    subject: &'a str,
    body: &'a str,
    date: String,
    iso: &'a MachineTimestamp,
    is_merge: bool,
}

impl Serialize for ViewerCommitSummary {
    fn serialize<SerializerType>(
        &self,
        serializer: SerializerType,
    ) -> Result<SerializerType::Ok, SerializerType::Error>
    where
        SerializerType: Serializer,
    {
        ViewerCommitSummaryRef {
            id: &self.id,
            subject: &self.subject,
            body: &self.body,
            date: self.committed_at.display_minute(),
            iso: &self.committed_at,
            is_merge: self.is_merge,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
struct ViewerCommitSummaryFields {
    id: CommitId,
    subject: String,
    body: String,
    date: String,
    iso: MachineTimestamp,
    is_merge: bool,
}

impl<'de> Deserialize<'de> for ViewerCommitSummary {
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        let fields = ViewerCommitSummaryFields::deserialize(deserializer)?;
        let expected_date = fields.iso.display_minute();
        if fields.date != expected_date {
            return Err(DeserializerType::Error::custom(format!(
                "commit display timestamp `{}` does not match `{expected_date}`",
                fields.date
            )));
        }
        Ok(Self {
            id: fields.id,
            subject: fields.subject,
            body: fields.body,
            committed_at: fields.iso,
            is_merge: fields.is_merge,
        })
    }
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
    pub extensions: ExcludedExtensions,
    pub hidden_paths: Vec<RepositoryRelativePath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerFailureCode {
    RepositoryDirectoryNotFound,
    RepositoryDirectoryNotGitRepository,
    SourceUnavailable,
    RenderFailed,
}

impl ViewerFailureCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RepositoryDirectoryNotFound => "RepositoryDirectoryNotFound",
            Self::RepositoryDirectoryNotGitRepository => "RepositoryDirectoryNotGitRepository",
            Self::SourceUnavailable => "SourceUnavailable",
            Self::RenderFailed => "RenderFailed",
        }
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
    pub repository_name: ProjectName,
    pub branch: GitHead,
    pub upstream: GitRevision,
    pub command: ViewerCommandLine,
    pub files: Vec<ViewerFileSummary>,
    pub commits_label: String,
    #[serde(default)]
    pub commit_count: usize,
    pub commits: Vec<ViewerCommitSummary>,
    pub commit_selection: ViewerCommitSelection,
    pub footer: ViewerFooter,
    pub exclusions: Option<ViewerAppliedExclusions>,
}

// TODO: move this logic to a client context once a context to manage ViewerActiveView state is
// created
/// Makes decision to handle commit selection in UI
#[must_use]
pub fn make_commit_selection_action(
    commit_selection: &ViewerCommitSelection,
    tab_id: ViewerTabId,
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
        tab_id: ViewerTabId,
    },
    Broken {
        tab_id: ViewerTabId,
        code: ViewerFailureCode,
        message: String,
    },
    Error {
        tab_id: ViewerTabId,
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
    pub version: ViewerVersion,
    pub tabs: Vec<ViewerTab>,
    pub active: ViewerActiveState,
    pub preferences: ViewerPreferences,
    pub feedback: Option<ViewerFeedback>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cursor", rename_all = "snake_case")]
pub enum ViewerHistoryCursor {
    Newest,
    OlderThan {
        render_id: RenderHistoryId,
        page: HistoryPageNumber,
    },
    NewerThan {
        render_id: RenderHistoryId,
        page: HistoryPageNumber,
    },
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerHistoryEntry {
    pub id: RenderHistoryId,
    pub title: String,
    pub repository_name: ProjectName,
    pub kind: ViewerRecipeKind,
    pub range_label: String,
    pub rendered_at: MachineTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerHistoryCopyPayload {
    pub json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerHistoryPage {
    pub entries: Vec<ViewerHistoryEntry>,
    pub total_count: HistoryRenderCount,
    pub position: HistoryPagePosition,
    pub has_newer: bool,
    pub has_older: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerProjectDiffExclusions {
    pub project_name: ProjectName,
    pub extensions: ExcludedExtensions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerDiffExclusions {
    pub default_extensions: ExcludedExtensions,
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
    pub tab_id: ViewerTabId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectViewerCommit {
    pub tab_id: ViewerTabId,
    pub id: CommitId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenViewerHistory {
    pub render_id: RenderHistoryId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetViewerHistoryCopy {
    pub render_id: RenderHistoryId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenViewerDiffFile {
    pub identity: ViewerViewIdentity,
    pub file: ViewerDiffFileId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerSyntaxClass {
    Keyword,
    String,
    Comment,
    Type,
    Function,
    Number,
    Constant,
    Operator,
    Tag,
    Variable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerCodeSpan {
    pub byte_start: usize,
    pub byte_end: usize,
    pub syntax_class: Option<ViewerSyntaxClass>,
    pub changed: bool,
}

impl ViewerCodeSpan {
    /// Returns this span's text when its byte range is valid for `line`.
    #[must_use]
    pub fn text<'line>(&self, line: &'line str) -> Option<&'line str> {
        line.get(self.byte_start..self.byte_end)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerCodeLine {
    pub text: String,
    pub spans: Vec<ViewerCodeSpan>,
    pub long_line_character_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerUnifiedSourceRow {
    pub old_line_number: Option<u32>,
    pub new_line_number: Option<u32>,
    pub code: ViewerCodeLine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerUnifiedRow {
    Meta(String),
    Hunk(String),
    Context(ViewerUnifiedSourceRow),
    Added(ViewerUnifiedSourceRow),
    Removed(ViewerUnifiedSourceRow),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerSplitCell {
    pub line_number: u32,
    pub code: ViewerCodeLine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerSplitRow {
    Meta(String),
    Hunk(String),
    Context {
        old_line_number: u32,
        new_line_number: u32,
        code: ViewerCodeLine,
    },
    Pair {
        old: Option<ViewerSplitCell>,
        new: Option<ViewerSplitCell>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerRows {
    Unified(Vec<ViewerUnifiedRow>),
    Split(Vec<ViewerSplitRow>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerFileRows {
    pub rows: ViewerRows,
    pub line_number_digits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerFileFailureCode {
    SourceUnavailable,
    ParseFailed,
    RowTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerRowEvent {
    FileStarted {
        file: ViewerDiffFileId,
    },
    UnifiedRows {
        file: ViewerDiffFileId,
        rows: Vec<ViewerUnifiedRow>,
    },
    SplitRows {
        file: ViewerDiffFileId,
        rows: Vec<ViewerSplitRow>,
    },
    FileFinished {
        file: ViewerDiffFileId,
        line_number_digits: u32,
    },
    FileFailed {
        file: ViewerDiffFileId,
        code: ViewerFileFailureCode,
        message: String,
        retryable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerRowStreamItem {
    pub identity: ViewerViewIdentity,
    pub sequence: u64,
    pub event: ViewerRowEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamViewerRows {
    pub identity: ViewerViewIdentity,
    pub file: Option<ViewerDiffFileId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "preference", content = "value", rename_all = "snake_case")]
pub enum SetViewerPreference {
    Layout(ViewerDiffLayout),
    Density(ViewerDiffDensity),
    Theme(ViewerTheme),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerStateChanged {
    pub version: ViewerVersion,
}
