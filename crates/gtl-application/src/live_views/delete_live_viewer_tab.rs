use gtl_models::viewer::ViewerTabId;
use rusqlite::Connection;

use super::persistence;
use crate::viewer::{
    ViewerState, ViewerStateError,
    session::CloseOutcome,
    work::{self, ReserveRecipeError, ReservedRecipeWork},
};

#[derive(Debug, thiserror::Error)]
pub enum DeleteLiveViewerTabError {
    #[error("live viewer tab is not available")]
    UnknownTab,
    #[error(transparent)]
    ViewerState(#[from] ViewerStateError),
    #[error(transparent)]
    ReserveWork(#[from] ReserveRecipeError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Deletes a saved live view and closes its matching viewer tab.
#[cqrsy::command]
pub fn execute(
    tab_id: ViewerTabId,
    connection: &Connection,
    viewer_state: &ViewerState,
) -> Result<Option<ReservedRecipeWork>, DeleteLiveViewerTabError> {
    viewer_state.update(|session| {
        let source = session
            .live_source(tab_id)
            .ok_or(DeleteLiveViewerTabError::UnknownTab)?;
        persistence::delete_live_view(connection, &source)?;
        let close = session
            .close(tab_id)
            .ok_or(DeleteLiveViewerTabError::UnknownTab)?;
        match close {
            CloseOutcome::ActiveChanged => {
                work::reserve_active_if_needed(session).map_err(Into::into)
            }
            CloseOutcome::ActiveUnchanged => Ok(None),
        }
    })?
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::{ViewerTabId, ViewerTabKind};
    use rusqlite::Connection;

    use super::DeleteLiveViewerTabError;
    use crate::{
        live_views::{delete_live_viewer_tab, list_live_views, persistence::store_test},
        recipes::{RecipeOp, RecipeTarget},
        utils,
        viewer::{ViewerState, work},
    };

    fn seed_live_view(connection: &Connection) {
        connection
            .execute(
                "INSERT INTO live_views \
                 (source_kind, source_value, display_name, created_at) \
                 VALUES ('LocalRepo', '/repos/project', 'project', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("seed live view");
    }

    fn open_live_tab(viewer: &ViewerState) -> ViewerTabId {
        let recipe = utils::viewer::recipe(RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        });
        work::reserve_open(
            viewer,
            recipe,
            gtl_models::recipes::RecipeBatchId::generate(),
            ViewerTabKind::Live,
        )
        .expect("live tab opens")
        .ticket()
        .tab_id
    }

    #[test]
    fn delete_removes_the_saved_view_and_its_tab_together() {
        let connection = store_test();
        seed_live_view(&connection);
        let viewer = ViewerState::new();
        let tab_id = open_live_tab(&viewer);

        let refresh =
            delete_live_viewer_tab::execute(tab_id, &connection, &viewer).expect("delete succeeds");

        assert!(refresh.is_none());
        assert!(
            list_live_views::execute(list_live_views::ListLiveViews, &connection)
                .expect("list saved views")
                .is_empty()
        );
        assert!(
            viewer
                .inspect(|session| session.tab(tab_id).is_none())
                .expect("viewer remains available")
        );
    }

    #[test]
    fn persistence_failure_leaves_the_live_tab_open() {
        let connection = store_test();
        connection
            .execute("DROP TABLE live_views", [])
            .expect("drop live views table");
        let viewer = ViewerState::new();
        let tab_id = open_live_tab(&viewer);

        let error = delete_live_viewer_tab::execute(tab_id, &connection, &viewer)
            .expect_err("delete failure rejects");

        assert!(matches!(error, DeleteLiveViewerTabError::Unexpected(_)));
        assert!(
            viewer
                .inspect(|session| session.tab(tab_id).is_some())
                .expect("viewer remains available")
        );
    }
}
