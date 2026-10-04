use std::path::Path;

use gtl_models::{
    diffs::{DiffTextId, DiffViewTitle},
    failure::{DiffTextFailure, ErrorMeta},
    paths::ProjectName,
    viewer::DiffLayout,
};

use crate::{
    ports::{
        ArtifactStore, Clock, DiffTextReader, HtmlRenderer, PlacedArtifact, UserSettingsLoadError,
        UserSettingsReader,
    },
    shared::notes::Note,
};

const STANDALONE_ARTIFACTS_MAX: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderTextDiff {
    pub id: DiffTextId,
    pub label: ProjectName,
    pub name: Option<ProjectName>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderTextDiffOk {
    pub artifact: PlacedArtifact,
    pub notes: Vec<Note>,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum RenderTextDiffError {
    #[error("diff text {0} is no longer stored")]
    #[meta(failure = DiffTextFailure::Missing)]
    Missing(DiffTextId),
    #[error(transparent)]
    #[meta(failure)]
    Invalid(#[from] DiffTextFailure),
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

/// Renders stored diff text into a standalone artifact under the application data root.
#[cqrsy::command]
pub fn execute(
    request: RenderTextDiff,
    data_root: &Path,
    app_settings: &impl UserSettingsReader,
    texts: &impl DiffTextReader,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderTextDiffOk, RenderTextDiffError> {
    let RenderTextDiff { id, label, name } = request;
    let text = texts
        .diff_text(&id)?
        .ok_or(RenderTextDiffError::Missing(id))?;
    let mut view =
        super::text_diff::view(&text, label, &gtl_models::diffs::ExtensionFilter::default())?;
    if let Some(name) = name {
        view.title = DiffViewTitle::Named { name };
    }
    let settings = app_settings.load()?;
    let html = renderer.build_html(
        &view,
        settings
            .viewer_render_options()
            .with_layout(DiffLayout::Unified),
        settings.theme(),
        settings.language(),
    )?;
    let artifact = store.place_standalone(
        &super::artifacts::text_root(data_root),
        &clock.now().map_err(anyhow::Error::from)?,
        &html,
        STANDALONE_ARTIFACTS_MAX,
    )?;
    let notes = vec![
        Note::info(format!(
            "diff-artifact: {}, {} file(s)",
            view.origin.name(),
            view.files.len()
        )),
        Note::info(format!("wrote {}", artifact.path().display())),
    ];
    Ok(RenderTextDiffOk { artifact, notes })
}
