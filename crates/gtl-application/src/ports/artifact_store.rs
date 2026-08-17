use std::path::Path;

use gtl_models::{
    artifacts::{
        ArtifactByteSize, ArtifactCommitRange, ArtifactContentHash, ArtifactDiffIdentity,
        RepositoryStoreId,
    },
    diffs::{DiffKind, ExcludedExtensions},
    paths::{AbsoluteFilePath, ProjectName, RepositoryRoot},
    timestamps::MachineTimestamp,
    viewer::{RenderOptions, Theme},
};

/// Everything the store needs to record one rendered artifact. `generated_at` is
/// supplied by the caller (via [`crate::ports::Clock`]) so placement stays deterministic in tests.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactMeta {
    pub repo_root: RepositoryRoot,
    pub repo_name: ProjectName,
    pub identity: ArtifactDiffIdentity,
    pub range_label: String,
    pub head_committed_at: Option<MachineTimestamp>,
    pub generated_at: MachineTimestamp,
    pub title: String,
    pub render_options: RenderOptions,
    /// The configured renderer theme used to build the artifact. `None` means
    /// the renderer selected its default theme.
    pub theme: Option<Theme>,
    /// The extension set that was in force when the artifact rendered (normalized,
    /// sorted; empty = unfiltered). Part of the range-reuse key: an artifact is
    /// only reusable by a render running under the same filter.
    pub excluded_extensions: ExcludedExtensions,
}

/// Values that must match before a stored commit-range artifact can be reused.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtifactRangeKey {
    pub range: ArtifactCommitRange,
    pub render_options: RenderOptions,
    pub theme: Option<Theme>,
    pub excluded_extensions: ExcludedExtensions,
}

/// Reports whether placement created an artifact or reused an identical one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacedArtifact {
    /// Placement wrote a new artifact.
    Created { path: AbsoluteFilePath },
    /// Placement found an identical stored artifact.
    Reused { path: AbsoluteFilePath },
}

impl PlacedArtifact {
    /// Returns the placed artifact's filesystem path.
    pub fn path(&self) -> &AbsoluteFilePath {
        match self {
            Self::Created { path } | Self::Reused { path } => path,
        }
    }

    /// Consumes the placement and returns its filesystem path.
    pub fn into_path(self) -> AbsoluteFilePath {
        match self {
            Self::Created { path } | Self::Reused { path } => path,
        }
    }

    /// Reports whether placement reused an identical artifact.
    pub const fn is_reused(&self) -> bool {
        matches!(self, Self::Reused { .. })
    }
}

/// One row from the content-addressed store's history listing - the port-facing
/// mirror of infra's private `Sidecar`, same reasoning as `ArtifactMeta`/`place`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRecord {
    pub repo_id: RepositoryStoreId,
    pub repo_name: ProjectName,
    pub title: String,
    pub range_label: String,
    pub head_committed_at: Option<MachineTimestamp>,
    pub generated_at: MachineTimestamp,
    pub content_hash: ArtifactContentHash,
    pub kind: DiffKind,
    pub byte_size: ArtifactByteSize,
}

/// The content-addressed store behind diff artifacts.
pub trait ArtifactStore: Clone + Send + Sync + 'static {
    /// Place `html` and its metadata under `store_root`, addressed by content hash.
    /// Idempotent per content hash: identical HTML reuses the existing artifact.
    fn place(
        &self,
        store_root: &Path,
        meta: &ArtifactMeta,
        html: &str,
    ) -> anyhow::Result<PlacedArtifact>;

    /// Find an existing artifact for a validated immutable commit range rendered under the same
    /// renderer layout, density, theme, and exclusion set, or `None` on a miss.
    fn lookup_by_range(
        &self,
        store_root: &Path,
        repo_root: &RepositoryRoot,
        key: &ArtifactRangeKey,
    ) -> anyhow::Result<Option<AbsoluteFilePath>>;

    /// Every recorded artifact under `store_root`, across all repos, unordered
    /// (the `history/list` handler owns sort order).
    fn list_history(&self, store_root: &Path) -> anyhow::Result<Vec<HistoryRecord>>;
}
