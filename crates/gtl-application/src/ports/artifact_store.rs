use std::path::Path;

use gtl_models::{
    artifacts::{ArtifactCommitRange, ArtifactDiffIdentity},
    diffs::ExcludedExtensions,
    paths::{AbsoluteFilePath, RepositoryRoot},
    settings::ViewerLanguage,
    timestamps::MachineTimestamp,
    viewer::{RenderOptions, Theme},
};

/// Everything the store needs to record one rendered artifact. `generated_at` is
/// supplied by the caller (via [`crate::ports::Clock`]) so placement stays deterministic in tests.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactMeta {
    pub repo_root: RepositoryRoot,
    pub identity: ArtifactDiffIdentity,
    pub generated_at: MachineTimestamp,
    pub render_options: RenderOptions,
    /// The configured renderer theme used to build the artifact. `None` means
    /// the renderer selected its default theme.
    pub theme: Option<Theme>,
    /// The language of the artifact's copy.
    pub language: ViewerLanguage,
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
    pub language: ViewerLanguage,
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
    #[must_use]
    pub fn path(&self) -> &AbsoluteFilePath {
        match self {
            Self::Created { path } | Self::Reused { path } => path,
        }
    }

    /// Consumes the placement and returns its filesystem path.
    #[must_use]
    pub fn into_path(self) -> AbsoluteFilePath {
        match self {
            Self::Created { path } | Self::Reused { path } => path,
        }
    }

    /// Reports whether placement reused an identical artifact.
    #[must_use]
    pub const fn is_reused(&self) -> bool {
        matches!(self, Self::Reused { .. })
    }
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
    /// renderer layout, density, theme, language, and exclusion set, or `None` on a miss.
    fn lookup_by_range(
        &self,
        store_root: &Path,
        repo_root: &RepositoryRoot,
        key: &ArtifactRangeKey,
    ) -> anyhow::Result<Option<AbsoluteFilePath>>;
}
