use std::path::{Path, PathBuf};

use domain::{diffs::DiffKind, viewer::RenderOptions};

/// Everything the store needs to record one rendered artifact. `generated_at` is
/// supplied by the caller (via [`crate::ports::Clock`]) so placement stays deterministic in tests.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactMeta {
    pub repo_root: PathBuf,
    pub repo_name: String,
    pub kind: DiffKind,
    pub base_sha: String,
    pub head_sha: String,
    pub range_label: String,
    pub head_committed_at: String,
    pub generated_at: String,
    pub title: String,
    pub render_options: RenderOptions,
    /// The configured renderer theme used to build the artifact. `None` means
    /// the renderer selected its default theme.
    pub theme: Option<String>,
    /// The extension set that was in force when the artifact rendered (normalized,
    /// sorted; empty = unfiltered). Part of the range-reuse key: an artifact is
    /// only reusable by a render running under the same filter.
    pub excluded_extensions: Vec<String>,
}

/// A placed artifact: where it landed and whether an identical one already existed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedArtifact {
    pub path: PathBuf,
    pub reused: bool,
}

/// One row from the content-addressed store's history listing - the port-facing
/// mirror of infra's private `Sidecar`, same reasoning as `ArtifactMeta`/`place`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRecord {
    pub repo_id: String,
    pub repo_name: String,
    pub title: String,
    pub range_label: String,
    pub head_committed_at: String,
    pub generated_at: String,
    pub content_hash: String,
    pub kind: DiffKind,
    pub byte_size: u64,
}

/// The content-addressed artifact store behind the diff previews.
pub trait ArtifactStore: Clone + Send + Sync + 'static {
    /// Place `html` and its metadata under `store_root`, addressed by content hash.
    /// Idempotent per content hash: identical HTML reuses the existing artifact.
    fn place(
        &self,
        store_root: &Path,
        meta: &ArtifactMeta,
        html: &str,
    ) -> anyhow::Result<PlacedArtifact>;

    /// Find an existing artifact for a pure commit range rendered under the same
    /// renderer layout, density, theme, and exclusion set, or `None` on a miss
    /// (always `None` for `WorkTree`, which is never range-addressable).
    #[allow(
        clippy::too_many_arguments,
        reason = "the explicit fields are the persisted range-reuse key"
    )]
    fn lookup_by_range(
        &self,
        store_root: &Path,
        repo_root: &Path,
        kind: DiffKind,
        base_sha: &str,
        head_sha: &str,
        render_options: RenderOptions,
        theme: Option<&str>,
        excluded_extensions: &[String],
    ) -> anyhow::Result<Option<PathBuf>>;

    /// Every recorded artifact under `store_root`, across all repos, unordered
    /// (the `history/list` handler owns sort order).
    fn list_history(&self, store_root: &Path) -> anyhow::Result<Vec<HistoryRecord>>;
}
