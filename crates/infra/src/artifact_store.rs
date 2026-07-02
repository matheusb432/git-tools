//! The content-addressed [`ArtifactStore`] adapter over the `store` module.
//!
//! Canonicalization, repo-id derivation, and sidecar field mapping match the
//! former `cli::commands::store_artifact` / `range_fast_path` byte-for-byte, so
//! artifacts placed before and after the extraction dedup against each other.

use std::path::{Path, PathBuf};

use application::ports::{ArtifactMeta, ArtifactStore, PlacedArtifact};
use domain::diffs::DiffKind;

/// The default store adapter: places artifacts into and looks them up out of the
/// on-disk content-addressed store.
#[derive(Debug, Clone, Copy, Default)]
pub struct StoreArtifacts;

impl ArtifactStore for StoreArtifacts {
    fn place(
        &self,
        store_root: &Path,
        meta: &ArtifactMeta,
        html: &str,
    ) -> anyhow::Result<PlacedArtifact> {
        let canonical =
            std::fs::canonicalize(&meta.repo_root).unwrap_or_else(|_| meta.repo_root.clone());
        let root_commit = crate::git_capture::root_commit(&meta.repo_root);
        let repo_id = crate::store::repo_id(root_commit.as_deref(), &canonical);
        let sidecar = crate::store::Sidecar {
            repo_id: repo_id.clone(),
            repo_name: meta.repo_name.clone(),
            repo_root: meta.repo_root.to_string_lossy().into_owned(),
            kind: meta.kind,
            base_sha: meta.base_sha.clone(),
            head_sha: meta.head_sha.clone(),
            range_label: meta.range_label.clone(),
            head_committed_at: meta.head_committed_at.clone(),
            generated_at: meta.generated_at.clone(),
            title: meta.title.clone(),
            byte_size: html.len() as u64,
        };
        let placed = crate::store::place(store_root, &repo_id, html, &sidecar)?;
        Ok(PlacedArtifact {
            path: placed.path,
            reused: placed.reused,
        })
    }

    fn lookup_by_range(
        &self,
        store_root: &Path,
        repo_root: &Path,
        kind: DiffKind,
        base_sha: &str,
        head_sha: &str,
    ) -> anyhow::Result<Option<PathBuf>> {
        let canonical =
            std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
        let repo_id = crate::store::repo_id(
            crate::git_capture::root_commit(repo_root).as_deref(),
            &canonical,
        );
        crate::store::lookup_by_range(store_root, &repo_id, kind, base_sha, head_sha)
    }
}
