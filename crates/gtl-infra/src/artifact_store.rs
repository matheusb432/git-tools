//! The content-addressed [`ArtifactStore`] adapter over the `store` module.
//!
//! Canonicalization, repo-id derivation, and sidecar field mapping match the
//! former `cli::commands::store_artifact` / `range_fast_path` byte-for-byte, so
//! artifacts placed before and after the extraction dedup against each other.

use std::path::Path;

use gtl_application::ports::{
    ArtifactMeta, ArtifactRangeKey, ArtifactStore, HistoryRecord, PlacedArtifact,
};
use gtl_models::{artifacts::ArtifactByteSize, paths::RepositoryRoot};

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
        let canonical = std::fs::canonicalize(&meta.repo_root)
            .unwrap_or_else(|_| meta.repo_root.as_ref().to_path_buf());
        let root_commit = crate::git_capture::root_commit(meta.repo_root.as_ref());
        let repo_id = crate::store::repo_id(root_commit.as_ref(), &canonical);
        let byte_size = u64::try_from(html.len()).map_err(anyhow::Error::from)?;
        let metadata = crate::store::ArtifactMetadata {
            repo_id: repo_id.clone(),
            repo_name: meta.repo_name.clone(),
            repo_root: meta.repo_root.clone(),
            identity: meta.identity.clone(),
            range_label: meta.range_label.clone(),
            head_committed_at: meta.head_committed_at.clone(),
            generated_at: meta.generated_at.clone(),
            title: meta.title.clone(),
            byte_size: ArtifactByteSize::new(byte_size),
            render_options: meta.render_options,
            theme: crate::store::ArtifactThemeMetadata::Recorded(meta.theme),
            language: meta.language,
            renderer_version: crate::store::RENDERER_VERSION,
            excluded_extensions: meta.excluded_extensions.clone(),
        };
        crate::store::place(store_root, &repo_id, html, &metadata)
    }

    fn lookup_by_range(
        &self,
        store_root: &Path,
        repo_root: &RepositoryRoot,
        key: &ArtifactRangeKey,
    ) -> anyhow::Result<Option<gtl_models::paths::AbsoluteFilePath>> {
        let canonical =
            std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.as_ref().to_path_buf());
        let repo_id = crate::store::repo_id(
            crate::git_capture::root_commit(repo_root.as_ref()).as_ref(),
            &canonical,
        );
        Ok(crate::store::lookup_by_range(store_root, &repo_id, key))
    }

    fn list_history(&self, store_root: &Path) -> anyhow::Result<Vec<HistoryRecord>> {
        let sidecars = crate::store::list_history_with_hash(store_root);
        Ok(sidecars
            .into_iter()
            .map(|(content_hash, metadata)| HistoryRecord {
                repo_id: metadata.repo_id,
                repo_name: metadata.repo_name,
                title: metadata.title,
                range_label: metadata.range_label,
                head_committed_at: metadata.head_committed_at,
                generated_at: metadata.generated_at,
                content_hash,
                kind: metadata.identity.kind(),
                byte_size: metadata.byte_size,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        artifacts::{ArtifactDiffIdentity, RepositoryStoreId},
        diffs::{DiffKind, ExcludedExtensions},
        timestamps::MachineTimestamp,
        viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
    };

    use super::*;
    use crate::{store::Sidecar, testing::pinned_range};

    #[test]
    fn place_persists_render_options_from_artifact_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let meta = ArtifactMeta {
            repo_root: RepositoryRoot::try_new(dir.path().to_path_buf()).unwrap(),
            repo_name: "git-tools".try_into().unwrap(),
            identity: ArtifactDiffIdentity::from_parts(
                DiffKind::TwoDot,
                Some(pinned_range("a", "b")),
            )
            .unwrap(),
            range_label: "main..HEAD".into(),
            head_committed_at: Some(MachineTimestamp::try_from("2026-07-03T00:00:00Z").unwrap()),
            generated_at: MachineTimestamp::try_from("2026-07-03T00:01:00Z").unwrap(),
            title: "diff".into(),
            render_options: RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            theme: Some(Theme::Dark),
            language: gtl_models::settings::ViewerLanguage::PtBr,
            excluded_extensions: ExcludedExtensions::default(),
        };

        let placed = StoreArtifacts
            .place(dir.path(), &meta, "<html></html>")
            .unwrap();
        let sidecar: Sidecar = serde_json::from_str(
            &std::fs::read_to_string(placed.path().with_extension("json")).unwrap(),
        )
        .unwrap();

        assert_eq!(sidecar.layout, "split");
        assert_eq!(sidecar.density, "full");
        assert_eq!(sidecar.language, "pt-BR");
    }

    #[test]
    fn list_history_reads_back_placed_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let repo_id = RepositoryStoreId::try_new("0123456789abcdef".to_owned()).unwrap();
        let sidecar = Sidecar {
            repo_id: repo_id.to_string(),
            repo_name: "git-tools".into(),
            repo_root: "/r".into(),
            kind: DiffKind::TwoDot,
            base_sha: "a".repeat(40),
            head_sha: "b".repeat(40),
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
            language: "en-US".into(),
            renderer_version: crate::store::RENDERER_VERSION,
        };
        let metadata = sidecar.try_into_metadata().unwrap();
        crate::store::place(dir.path(), &repo_id, "<html></html>", &metadata).unwrap();
        let expected_hash = crate::store::content_hash("<html></html>");

        let entries = StoreArtifacts.list_history(dir.path()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content_hash, expected_hash);
        assert_eq!(entries[0].repo_id, repo_id);
        assert_eq!(entries[0].repo_name.as_str(), "git-tools");
        assert_eq!(entries[0].kind, DiffKind::TwoDot);
        assert_eq!(entries[0].byte_size, ArtifactByteSize::new(42));
    }
}
