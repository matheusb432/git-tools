use std::path::{Path, PathBuf};

use gtl_models::{
    artifacts::ArtifactDiffIdentity, diffs::ExtensionFilter, paths::RepositoryRoot,
    settings::UserSettings, viewer::DiffLayout,
};

use crate::ports::{ArtifactMeta, ArtifactStore, Clock, HtmlRenderer, PlacedArtifact};

pub(crate) fn root(repo_root: &Path) -> PathBuf {
    repo_root.join(".artifacts").join("gtl")
}

pub(crate) fn place_tabbed_artifact(
    repo_root: RepositoryRoot,
    views: &[super::View],
    title_label: &str,
    settings: &UserSettings,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> anyhow::Result<PlacedArtifact> {
    let store_root = root(&repo_root);
    let generated_at = clock.now().map_err(anyhow::Error::from)?;
    let title = super::batch::dated_title(&generated_at, title_label);
    let render_options = settings
        .viewer_render_options()
        .with_layout(DiffLayout::Unified);
    let theme = settings.theme();
    let language = settings.language();
    let html = renderer.build_tabbed_html(&title, views, render_options, theme, language)?;
    let meta = ArtifactMeta {
        repo_root,
        identity: ArtifactDiffIdentity::WorkTree,
        generated_at,
        render_options,
        theme,
        language,
        extension_filter: ExtensionFilter::default(),
    };
    store.place(&store_root, &meta, &html)
}
