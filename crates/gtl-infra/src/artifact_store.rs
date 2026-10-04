//! The content-addressed [`ArtifactStore`] adapter over the `store` module.
//!
//! Canonicalization, repo-id derivation, and sidecar field mapping match the
//! former `cli::commands::store_artifact` / `range_fast_path` byte-for-byte, so
//! artifacts placed before and after the extraction dedup against each other.

use std::path::Path;

use gtl_application::ports::{ArtifactMeta, ArtifactRangeKey, ArtifactStore, PlacedArtifact};
use gtl_models::{paths::RepositoryRoot, timestamps::MachineTimestamp};

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
        let metadata = crate::store::ArtifactMetadata {
            repo_id: repo_id.clone(),
            identity: meta.identity.clone(),
            render_options: meta.render_options,
            theme: crate::store::ArtifactThemeMetadata::Recorded(meta.theme),
            language: meta.language,
            renderer_version: crate::store::RENDERER_VERSION,
            extension_filter: meta.extension_filter.clone(),
        };
        crate::store::place(store_root, &repo_id, html, &meta.generated_at, &metadata)
    }

    fn place_standalone(
        &self,
        store_root: &Path,
        generated_at: &MachineTimestamp,
        html: &str,
        retained_max: usize,
    ) -> anyhow::Result<PlacedArtifact> {
        crate::store::place_standalone(store_root, generated_at, html, retained_max)
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
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        artifacts::ArtifactDiffIdentity,
        diffs::{DiffKind, ExtensionFilter},
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
            identity: ArtifactDiffIdentity::from_parts(
                DiffKind::TwoDot,
                Some(pinned_range("a", "b")),
            )
            .unwrap(),
            generated_at: MachineTimestamp::try_from("2026-07-03T00:01:00Z").unwrap(),
            render_options: RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            theme: Some(Theme::Dark),
            language: gtl_models::settings::ViewerLanguage::PtBr,
            extension_filter: ExtensionFilter::default(),
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
}
