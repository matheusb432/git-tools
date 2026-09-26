use gtl_models::{
    failure::{ErrorMeta, Failure, ViewerFailure},
    viewer::DiffDensity,
};
use gtl_wire::viewer::ViewerViewIdentity;

use super::{ViewerState, ViewerStateError, session::ActiveContentSnapshot, shell};
use crate::{
    diffs::FullContextDiffState,
    ports::{UserSettingsLoadError, UserSettingsReader},
};

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ViewerSourceError {
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] ViewerStateError),
    #[error("the viewer identity changed")]
    #[meta(failure = Failure::Changed)]
    Changed,
    #[error("the diff source is still being prepared")]
    #[meta(failure = ViewerFailure::SourcePreparing)]
    Preparing,
}

pub(super) fn current(
    identity: ViewerViewIdentity,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
) -> Result<ActiveContentSnapshot, ViewerSourceError> {
    let options = settings.load()?.viewer_render_options();
    shell::content_snapshot_for_identity(state, identity, options)?
        .ok_or(ViewerSourceError::Changed)
}

pub(super) fn ready(
    identity: ViewerViewIdentity,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
) -> Result<ActiveContentSnapshot, ViewerSourceError> {
    let settings = settings.load()?;
    let options = settings.viewer_render_options();
    let snapshot = shell::content_snapshot_for_identity(state, identity, options)?
        .ok_or(ViewerSourceError::Changed)?;
    if options.density() == DiffDensity::Full
        && matches!(
            snapshot.view().full_context,
            FullContextDiffState::Deferred(_)
        )
    {
        return Err(ViewerSourceError::Preparing);
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gtl_models::{
        diffs::ExtensionSelection,
        git::GitDiffSpec,
        settings::UserSettings,
        viewer::{DiffLayout, RenderOptions},
    };

    use super::*;
    use crate::{
        diffs::FullContextDiffSource,
        recipes::{RecipeBatchId, RecipeOp},
        utils::{self, FixedUserSettingsStore},
        viewer::session::CachedView,
    };

    #[test]
    fn metadata_is_available_while_rows_wait_for_full_source() {
        let state = ViewerState::new();
        let options = RenderOptions::new(DiffLayout::Unified, DiffDensity::Full);
        let settings = FixedUserSettingsStore::new(
            UserSettings::default().with_viewer_render_options(options),
        );
        let identity = state
            .update(|session| {
                let tab = session
                    .open(
                        utils::viewer::recipe(RecipeOp::MergeDiff {
                            base: None,
                            pinned: None,
                        }),
                        RecipeBatchId::generate(),
                    )
                    .unwrap();
                let ticket = session.begin_compute(tab).unwrap();
                let mut view = utils::viewer::empty_view();
                view.full_context = FullContextDiffState::Deferred(FullContextDiffSource::new(
                    GitDiffSpec::AgainstWorkingTree(utils::git_revision("HEAD")),
                    ExtensionSelection::all(),
                ));
                session.publish_labeled_if_current(
                    ticket,
                    CachedView::new(Arc::new(view)),
                    crate::utils::viewer::label("current"),
                );
                shell::identity_for(session.active_content_identity().unwrap(), options)
            })
            .unwrap();
        assert!(current(identity, &state, &settings).is_ok());
        assert!(matches!(
            ready(identity, &state, &settings),
            Err(ViewerSourceError::Preparing)
        ));
        let mut outdated_identity = identity;
        outdated_identity.selection_generation =
            gtl_models::viewer::ViewerSelectionGeneration::new(999);
        assert!(matches!(
            current(outdated_identity, &state, &settings),
            Err(ViewerSourceError::Changed)
        ));
        assert!(matches!(
            ready(outdated_identity, &state, &settings),
            Err(ViewerSourceError::Changed)
        ));
    }
}
