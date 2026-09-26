use gtl_models::{
    failure::{ErrorMeta, Failure, Resource, ViewerFailure},
    viewer::ViewerTabState,
};
use gtl_wire::viewer::SetViewerTabPinned;

use super::{ViewerState, ViewerStateError};

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum SetViewerTabPinnedError {
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] ViewerStateError),
    #[error("wait for the snapshot to finish before pinning it")]
    #[meta(failure = ViewerFailure::SnapshotPending)]
    SnapshotPending,
    #[error("viewer tab is not available")]
    #[meta(failure = Failure::Gone { resource: Resource::ViewerTab })]
    UnknownTab,
}

/// Pins or unpins a tab; a tab that is not live must finish rendering so its pin keeps the
/// reviewed commits.
#[cqrsy::command]
pub fn execute(
    request: SetViewerTabPinned,
    state: &ViewerState,
) -> Result<(), SetViewerTabPinnedError> {
    state.update(|session| {
        let tab = session
            .tab(request.tab_id)
            .ok_or(SetViewerTabPinnedError::UnknownTab)?;
        if request.pinned && !tab.tab.live() && !matches!(tab.tab.state(), ViewerTabState::Ready) {
            return Err(SetViewerTabPinnedError::SnapshotPending);
        }
        session.set_pinned(request.tab_id, request.pinned);
        Ok(())
    })?
}

#[cfg(test)]
mod tests {
    use gtl_models::recipes::RecipeBatchId;

    use super::*;
    use crate::{
        recipes::{RecipeOp, RecipeTarget},
        utils,
        viewer::{set_viewer_tab_pinned, work},
    };

    #[test]
    fn a_rendering_snapshot_cannot_be_pinned() {
        let state = ViewerState::new();
        let work = work::reserve_open(
            &state,
            utils::viewer::recipe(RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            }),
            RecipeBatchId::generate(),
        )
        .unwrap();
        let tab_id = work.ticket().tab_id;

        let result = set_viewer_tab_pinned::execute(
            SetViewerTabPinned {
                tab_id,
                pinned: true,
            },
            &state,
        );

        assert!(matches!(
            result,
            Err(SetViewerTabPinnedError::SnapshotPending)
        ));
        assert!(
            !state
                .inspect(|session| session.tab(tab_id).unwrap().pinned)
                .unwrap()
        );
    }
}
