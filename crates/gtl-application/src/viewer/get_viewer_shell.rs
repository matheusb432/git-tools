use gtl_models::failure::ErrorMeta;
use gtl_wire::viewer::ViewerShell;

use super::{
    ViewerState, ViewerStateError,
    ensure_view_full_context::{self, ReservedFullContext},
    shell::{self, ProjectViewerShellError},
    work::{self, ReservedCommitWork},
};
use crate::ports::{UserSettingsLoadError, UserSettingsReader};

pub struct GetViewerShellOk {
    pub shell: ViewerShell,
    pub commit_reload: Option<ReservedCommitWork>,
    pub full_context: Option<ReservedFullContext>,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum GetViewerShellError {
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] ViewerStateError),
    #[error(transparent)]
    #[meta(transparent)]
    Projection(#[from] ProjectViewerShellError),
    #[error(transparent)]
    #[meta(transparent)]
    CommitReload(#[from] work::ReserveCommitError),
}

#[cqrsy::query]
pub fn execute(
    state: &ViewerState,
    settings: &impl UserSettingsReader,
) -> Result<GetViewerShellOk, GetViewerShellError> {
    let settings = settings.load()?;
    let commit_reload = work::reserve_selected_commit_reload(state)?;
    let full_context = ensure_view_full_context::reserve(state, settings.viewer_render_options())?;
    let shell = state.inspect(|session| shell::project(session, &settings))??;
    Ok(GetViewerShellOk {
        shell,
        commit_reload,
        full_context,
    })
}
