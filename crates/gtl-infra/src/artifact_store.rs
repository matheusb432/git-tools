//! The content-addressed [`ArtifactStore`] adapter over the `store` module.
//!
//! Canonicalization, repo-id derivation, and sidecar field mapping match the
//! former `cli::commands::store_artifact` / `range_fast_path` byte-for-byte, so
//! artifacts placed before and after the extraction dedup against each other.

use std::path::{Path, PathBuf};

use gtl_application::ports::{
    ArtifactMeta, ArtifactRangeKey, ArtifactStore, HistoryRecord, PlacedArtifact,
};

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
            layout: meta.render_options.layout().to_string(),
            density: meta.render_options.density().to_string(),
            theme: meta.theme.clone(),
            theme_recorded: true,
            renderer_version: crate::store::RENDERER_VERSION,
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
        key: &ArtifactRangeKey,
    ) -> anyhow::Result<Option<PathBuf>> {
        let canonical =
            std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
        let repo_id = crate::store::repo_id(
            crate::git_capture::root_commit(repo_root).as_deref(),
            &canonical,
        );
        Ok(crate::store::lookup_by_range(store_root, &repo_id, key))
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
    use gtl_models::{
        diffs::DiffKind,
        viewer::{DiffDensity, DiffLayout, RenderOptions},
    };

    use super::*;
    use crate::store::Sidecar;

    #[test]
    fn place_persists_render_options_from_artifact_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let meta = ArtifactMeta {
            repo_root: dir.path().to_path_buf(),
            repo_name: "git-tools".into(),
            kind: DiffKind::TwoDot,
            base_sha: "aaa".into(),
            head_sha: "bbb".into(),
            range_label: "main..HEAD".into(),
            head_committed_at: "2026-07-03T00:00:00Z".into(),
            generated_at: "2026-07-03T00:01:00Z".into(),
            title: "diff".into(),
            render_options: RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            theme: Some("dark".into()),
            excluded_extensions: Vec::new(),
        };

        let placed = StoreArtifacts
            .place(dir.path(), &meta, "<html></html>")
            .unwrap();
        let sidecar: Sidecar = serde_json::from_str(
            &std::fs::read_to_string(placed.path.with_extension("json")).unwrap(),
        )
        .unwrap();

        assert_eq!(sidecar.layout, "split");
        assert_eq!(sidecar.density, "full");
    }

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
            layout: RenderOptions::DEFAULT.layout().to_string(),
            density: RenderOptions::DEFAULT.density().to_string(),
            theme: None,
            theme_recorded: true,
            renderer_version: crate::store::RENDERER_VERSION,
        };
        crate::store::place(dir.path(), "repo123", "<html></html>", &sidecar).unwrap();
        let expected_hash = crate::store::content_hash("<html></html>");

        let entries = StoreArtifacts.list_history(dir.path()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content_hash, expected_hash);
        assert_eq!(entries[0].repo_id, "repo123");
        assert_eq!(entries[0].repo_name, "git-tools");
        assert_eq!(entries[0].kind, DiffKind::TwoDot);
        assert_eq!(entries[0].byte_size, 42);
    }
}
