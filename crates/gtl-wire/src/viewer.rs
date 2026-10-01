//! Typed values exchanged by the desktop viewer and its Dioxus Web shell.

use gtl_models::{
    diffs::{CommitId, DiffLineCount, DiffViewTitle, ExtensionFilter, PinnedRange},
    failure::Failure,
    git::{GitHead, GitRevision},
    paths::{AbsoluteFilePath, ProjectName, RepositoryRelativePath, RepositoryRoot},
    recipes::RecipeLabel,
    settings::UserSettingsRevision,
    timestamps::MachineTimestamp,
    viewer::{
        HistoryPageNumber, HistoryPagePosition, HistoryRenderCount, RenderHistoryId,
        ViewerKeybindings, ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId,
        ViewerTabPlacement, ViewerVersion,
    },
};
use nutype::nutype;
use serde::{Deserialize, Serialize};

pub mod commit_search;
pub mod projects;
pub mod push;

pub const VIEWER_PROTOCOL_VERSION: u32 = 60;

pub mod file_filters;
pub const VIEWER_COMMIT_PAGE_MAX_ENTRIES: usize = 100;
pub const VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES: usize = 256 * 1024;
pub const VIEWER_COMMIT_BODY_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const VIEWER_ROW_BATCH_MAX_ROWS: usize = 64;
pub const VIEWER_ROW_RANGE_MAX_ROWS: usize = 512;
pub const VIEWER_ROW_SESSIONS_MAX: usize = 10;
pub const VIEWER_ROW_BATCH_MAX_ENCODED_BYTES: usize = 256 * 1024;
pub const VIEWER_ROW_MAX_ENCODED_BYTES: usize = 4 * 1024 * 1024;
pub const VIEWER_SEARCH_QUERY_MAX_BYTES: usize = 256;
pub const VIEWER_FILE_SEARCH_MAX_MATCHES: usize = 50_000;
pub const VIEWER_FILE_SEARCH_MAX_ENCODED_BYTES: usize = 1024 * 1024;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerTheme {
    #[default]
    Dark,
    Mirage,
    Glacier,
    Graphite,
    Carbon,
}

impl ViewerTheme {
    /// Returns the stable theme token used by serialized state and document roots.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Mirage => "mirage",
            Self::Glacier => "glacier",
            Self::Graphite => "graphite",
            Self::Carbon => "carbon",
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
    pub wrap_lines: bool,
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

/// SHA-256 identity of row sources and format, independent of stream authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ViewerRowContentId([u8; 32]);

impl ViewerRowContentId {
    #[must_use]
    pub const fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    #[must_use]
    pub const fn into_digest(self) -> [u8; 32] {
        self.0
    }
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
    #[serde(default)]
    pub details: Option<ViewerTabDetails>,
    #[serde(default)]
    pub custom_name: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    pub id: ViewerTabId,
    pub label: RecipeLabel,
    /// Whether the tab updates its snapshot whenever its source changes.
    pub live: bool,
    pub state: ViewerTabState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerTabDetails {
    pub repository: RepositoryRoot,
    /// The source comparison, independent of a custom tab name.
    pub comparison: RecipeLabel,
    pub range: Option<PinnedRange>,
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
    pub source_id: Option<ViewerRowContentId>,
    pub id: ViewerDiffFileId,
    pub path: RepositoryRelativePath,
    pub absolute_path: AbsoluteFilePath,
    pub anchor_id: String,
    pub added: DiffLineCount,
    pub removed: DiffLineCount,
    pub status: ViewerFileStatus,
    pub can_open_in_editor: bool,
    pub initially_expanded: bool,
    pub row_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct ViewerAppliedExtensionFilter {
    pub filter: ExtensionFilter,
    pub hidden_paths: Vec<RepositoryRelativePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ViewerCommitSelection {
    None,
    Pending { id: CommitId },
    Ready { id: CommitId },
    Error { id: CommitId, failure: Failure },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerRowSourceState {
    Ready,
    Pending,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerActiveView {
    #[serde(default)]
    pub modified_files: bool,
    pub identity: ViewerViewIdentity,
    pub content_id: ViewerRowContentId,
    pub row_source: ViewerRowSourceState,
    pub title: DiffViewTitle,
    pub repository_name: ProjectName,
    pub branch: GitHead,
    pub upstream: GitRevision,
    pub command: ViewerCommandLine,
    pub files: Vec<ViewerFileSummary>,
    #[serde(default)]
    pub commit_count: usize,
    pub commits: Vec<ViewerCommitSummary>,
    pub commit_selection: ViewerCommitSelection,
    pub footer: ViewerFooter,
    pub extension_filter: Option<ViewerAppliedExtensionFilter>,
    /// The tab shows only changes committed after this time.
    #[serde(default)]
    pub changes_since: Option<MachineTimestamp>,
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
    /// The live source is broken; updates resume when it recovers.
    Broken {
        tab_id: ViewerTabId,
        failure: Failure,
    },
    /// Computing or rendering the view failed.
    Error {
        tab_id: ViewerTabId,
        failure: Failure,
    },
    Ready {
        view: Box<ViewerActiveView>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerPreferences {
    pub accessibility: gtl_models::settings::ViewerAccessibility,
    pub language: gtl_models::settings::ViewerLanguage,
    pub date_format: gtl_models::settings::ViewerDateFormat,
    pub sidebars: gtl_models::viewer::ViewerSidebarVisibility,
    pub theme: ViewerTheme,
    pub render_options: ViewerRenderOptions,
    pub copy_with_line_context: bool,
    pub diff_files_sort: gtl_models::settings::DiffFilesSort,
    pub keybindings: ViewerKeybindings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewerFeedback {
    SnapshotRecipesSkipped { labels: Vec<RecipeLabel> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerShell {
    pub version: ViewerVersion,
    /// Latest explicit request to show the active diff, retained across background updates.
    pub focus_request_version: Option<ViewerVersion>,
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

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "filter", rename_all = "snake_case")]
pub enum ViewerHistoryFilter {
    #[default]
    All,
    Unassociated,
    Project {
        name: ProjectName,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListViewerHistory {
    pub cursor: ViewerHistoryCursor,
    pub filter: ViewerHistoryFilter,
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
    pub label: RecipeLabel,
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
    pub projects: Vec<ProjectName>,
    pub entries: Vec<ViewerHistoryEntry>,
    pub total_count: HistoryRenderCount,
    pub position: HistoryPagePosition,
    pub has_newer: bool,
    pub has_older: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerUserSettings {
    pub accessibility: gtl_models::settings::ViewerAccessibility,
    pub language: gtl_models::settings::ViewerLanguage,
    pub date_format: gtl_models::settings::ViewerDateFormat,
    pub revision: UserSettingsRevision,
    pub focus_window_on_diff: bool,
    pub copy_with_line_context: bool,
    pub diff_files_sort: gtl_models::settings::DiffFilesSort,
    pub sidebars: gtl_models::viewer::ViewerSidebarVisibility,
    pub projects_sort: gtl_models::settings::ProjectsSort,
    pub projects_page_size: gtl_models::settings::ProjectsPageSize,
    pub configuration_path: Option<String>,
    pub configured_theme: Option<ViewerTheme>,
    pub effective_theme: ViewerTheme,
    pub render_options: ViewerRenderOptions,
    pub push_confirmation: gtl_models::settings::PushConfirmationPreferences,
    pub viewer_push_no_confirmation_projects: Vec<ProjectName>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FieldUpdate<T> {
    Update(T),
    Clear,
    #[default]
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EditSettingsRequest {
    pub ui_scale_percent: FieldUpdate<gtl_models::settings::ViewerScalePercent>,
    pub reduce_motion: FieldUpdate<bool>,
    pub language: FieldUpdate<gtl_models::settings::ViewerLanguage>,
    pub date_format: FieldUpdate<gtl_models::settings::ViewerDateFormat>,
    pub expected_revision: Option<UserSettingsRevision>,
    pub focus_window_on_diff: FieldUpdate<bool>,
    pub files_sidebar_visible: FieldUpdate<bool>,
    pub commits_sidebar_visible: FieldUpdate<bool>,
    pub wrap_lines: FieldUpdate<bool>,
    pub copy_with_line_context: FieldUpdate<bool>,
    pub diff_files_sort: FieldUpdate<gtl_models::settings::DiffFilesSort>,
    pub projects_sort: FieldUpdate<gtl_models::settings::ProjectsSort>,
    pub projects_page_size: FieldUpdate<gtl_models::settings::ProjectsPageSize>,
    pub theme: FieldUpdate<ViewerTheme>,
    pub layout: FieldUpdate<ViewerDiffLayout>,
    pub density: FieldUpdate<ViewerDiffDensity>,
    pub push_confirmation_required: FieldUpdate<bool>,
    pub viewer_push_confirmation_required: FieldUpdate<bool>,
    pub viewer_push_no_confirmation_projects: FieldUpdate<Vec<ProjectName>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerTabRequest {
    pub tab_id: ViewerTabId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveViewerTab {
    pub tab_id: ViewerTabId,
    pub target_tab_id: ViewerTabId,
    pub placement: ViewerTabPlacement,
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
    /// Characters omitted from the source after the transmitted prefix; absent for complete lines.
    pub omitted_character_count: Option<usize>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerRowEvent {
    FileStarted {
        file: ViewerDiffFileId,
        row_count: u32,
        start_row: u32,
    },
    UnifiedRows {
        file: ViewerDiffFileId,
        start_row: u32,
        rows: Vec<ViewerUnifiedRow>,
    },
    SplitRows {
        file: ViewerDiffFileId,
        start_row: u32,
        rows: Vec<ViewerSplitRow>,
    },
    FileFinished {
        file: ViewerDiffFileId,
        line_number_digits: u32,
        end_row: u32,
    },
    FileFailed {
        file: ViewerDiffFileId,
        failure: Failure,
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
    pub row_range: Option<ViewerRowRange>,
}

/// A nonempty logical row window, bounded to one normal batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ViewerRowRangeFields", into = "ViewerRowRangeFields")]
pub struct ViewerRowRange {
    start: u32,
    count: u32,
}

#[derive(Serialize, Deserialize)]
struct ViewerRowRangeFields {
    start: u32,
    count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewerRowRangeError;

impl std::fmt::Display for ViewerRowRangeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("viewer row range exceeds supported bounds")
    }
}

impl std::error::Error for ViewerRowRangeError {}

impl ViewerRowRange {
    pub fn try_new(start: u32, count: u32) -> Result<Self, ViewerRowRangeError> {
        if count == 0
            || u64::from(count) > VIEWER_ROW_RANGE_MAX_ROWS as u64
            || start.checked_add(count).is_none()
        {
            return Err(ViewerRowRangeError);
        }
        Ok(Self { start, count })
    }

    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }

    #[must_use]
    pub const fn count(self) -> u32 {
        self.count
    }

    #[must_use]
    pub const fn end(self) -> u32 {
        self.start + self.count
    }
}

impl TryFrom<ViewerRowRangeFields> for ViewerRowRange {
    type Error = ViewerRowRangeError;
    fn try_from(fields: ViewerRowRangeFields) -> Result<Self, Self::Error> {
        Self::try_new(fields.start, fields.count)
    }
}

impl From<ViewerRowRange> for ViewerRowRangeFields {
    fn from(range: ViewerRowRange) -> Self {
        Self {
            start: range.start,
            count: range.count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchViewerFiles {
    pub identity: ViewerViewIdentity,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerFileSearchResult {
    pub identity: ViewerViewIdentity,
    pub files: Vec<ViewerDiffFileId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerDiffSearchDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewerDiffSearchMatch {
    pub file: ViewerDiffFileId,
    pub row_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindViewerDiff {
    pub identity: ViewerViewIdentity,
    /// Files to search, in navigation order.
    pub files: Vec<ViewerDiffFileId>,
    pub query: String,
    pub direction: ViewerDiffSearchDirection,
    pub anchor: Option<ViewerDiffSearchMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadViewerDiffText {
    pub identity: ViewerViewIdentity,
    pub file: ViewerDiffFileId,
    pub row_range: ViewerRowRange,
    pub old_side: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerDiffTextLine {
    pub line_number: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerDiffSearchResult {
    pub identity: ViewerViewIdentity,
    pub total_matches: u64,
    pub active_match: Option<ViewerDiffSearchMatch>,
    pub wrapped: bool,
    /// Searched files with at least one match, in navigation order.
    pub matched_files: Vec<ViewerDiffFileId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "preference", content = "value", rename_all = "snake_case")]
pub enum SetViewerPreference {
    Layout(ViewerDiffLayout),
    Density(ViewerDiffDensity),
    Theme(ViewerTheme),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchViewer {
    pub live_tab_id: Option<ViewerTabId>,
    pub projects: projects::ViewerProjectSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerLiveCheck {
    pub tab_id: ViewerTabId,
    pub elapsed_ms: u64,
    pub result: Result<(), Failure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerStateChanged {
    pub version: ViewerVersion,
    pub live_check: Option<ViewerLiveCheck>,
    pub project_status: Option<projects::ViewerProjectStatusUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerSettingsRecovery {
    pub configuration_path: String,
    pub diagnostic: Option<String>,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetSettings {
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetSettingsOk {
    pub backup_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetViewerModifiedFiles {
    pub tab_id: ViewerTabId,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SetViewerTabPinned {
    pub tab_id: ViewerTabId,
    pub pinned: bool,
}

/// Narrows one tab to changes committed after `changes_since`, or restores its whole range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetViewerChangesSince {
    pub tab_id: ViewerTabId,
    pub changes_since: Option<MachineTimestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SetViewerTabLive {
    pub tab_id: ViewerTabId,
    pub live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameViewerSnapshot {
    pub tab_id: ViewerTabId,
    pub name: String,
}
