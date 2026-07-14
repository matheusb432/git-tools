//! The content-addressed [`ArtifactStore`] adapter over the `store` module.
//!
//! Canonicalization, repo-id derivation, and sidecar field mapping match the
//! former `cli::commands::store_artifact` / `range_fast_path` byte-for-byte, so
//! artifacts placed before and after the extraction dedup against each other.

use std::path::{Path, PathBuf};

use application::ports::{ArtifactMeta, ArtifactStore, HistoryRecord, PlacedArtifact};
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
            excluded_extensions: meta.excluded_extensions.clone(),
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
        excluded_extensions: &[String],
    ) -> anyhow::Result<Option<PathBuf>> {
        let canonical =
            std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
        let repo_id = crate::store::repo_id(
            crate::git_capture::root_commit(repo_root).as_deref(),
            &canonical,
        );
        Ok(crate::store::lookup_by_range(
            store_root,
            &repo_id,
            kind,
            base_sha,
            head_sha,
            excluded_extensions,
        ))
    }

    fn list_history(&self, store_root: &Path) -> anyhow::Result<Vec<HistoryRecord>> {
        let sidecars = crate::store::list_history_with_hash(store_root);
        Ok(sidecars
            .into_iter()
            .map(|(content_hash, sidecar)| HistoryRecord {
                repo_id: sidecar.repo_id,
                repo_name: sidecar.repo_name,
                title: sidecar.title,
                range_label: sidecar.range_label,
                head_committed_at: sidecar.head_committed_at,
                generated_at: sidecar.generated_at,
                content_hash,
                kind: sidecar.kind,
                byte_size: sidecar.byte_size,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Sidecar;

    #[test]
    fn list_history_reads_back_placed_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = Sidecar {
            repo_id: "repo123".into(),
            repo_name: "git-tools".into(),
            repo_root: "/r".into(),
            kind: DiffKind::TwoDot,
            base_sha: "aaa".into(),
            head_sha: "bbb".into(),
            range_label: "main..HEAD".into(),
            head_committed_at: "2026-07-03T00:00:00Z".into(),
            generated_at: "2026-07-03T00:01:00Z".into(),
            excluded_extensions: Vec::new(),
            title: "diff".into(),
            byte_size: 42,
        };
        let placed = crate::store::place(dir.path(), "repo123", "<html></html>", &sidecar).unwrap();
        let expected_hash = placed.path.file_stem().unwrap().to_str().unwrap();

        let entries = StoreArtifacts.list_history(dir.path()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content_hash, expected_hash);
        assert_eq!(entries[0].repo_id, "repo123");
        assert_eq!(entries[0].repo_name, "git-tools");
        assert_eq!(entries[0].kind, DiffKind::TwoDot);
        assert_eq!(entries[0].byte_size, 42);
    }
}
