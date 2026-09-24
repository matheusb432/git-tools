use gtl_models::{
    failure::{ErrorMeta, Failure, Resource, ViewerFailure},
    viewer::{ViewerTabKind, ViewerTabState},
};
use gtl_wire::viewer::SetViewerTabPinned;
use rusqlite::{Connection, params};

use super::{
    ViewerState, ViewerStateError,
    session::ViewerSession,
    work::{self, ReservedRecipeWork},
};
use crate::recipes::Recipe;

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum PinnedTabsError {
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] ViewerStateError),
    #[error("wait for the snapshot to finish before pinning it")]
    #[meta(failure = ViewerFailure::SnapshotPending)]
    SnapshotPending,
    #[error("viewer tab is not available")]
    #[meta(failure = Failure::Gone { resource: Resource::ViewerTab })]
    UnknownTab,
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

#[cqrsy::command]
pub fn execute(
    request: SetViewerTabPinned,
    state: &ViewerState,
    connection: &mut Connection,
) -> Result<(), PinnedTabsError> {
    state.update(|session| {
        let tab = session
            .tab(request.tab_id)
            .ok_or(PinnedTabsError::UnknownTab)?;
        if request.pinned
            && tab.tab.kind() == ViewerTabKind::Snapshot
            && !matches!(tab.tab.state(), ViewerTabState::Ready)
        {
            return Err(PinnedTabsError::SnapshotPending);
        }
        let records = session
            .tabs()
            .filter(|tab| {
                if tab.tab.id() == request.tab_id {
                    request.pinned
                } else {
                    tab.pinned
                }
            })
            .map(|tab| (tab.recipe.clone(), tab.tab.kind() == ViewerTabKind::Live))
            .collect::<Vec<_>>();
        persist(connection, &records)?;
        session.set_pinned(request.tab_id, request.pinned);
        Ok(())
    })?
}

pub fn save_order(state: &ViewerState, connection: &mut Connection) -> Result<(), PinnedTabsError> {
    state.inspect(|session| persist(connection, &records(session)))?
}

fn records(session: &ViewerSession) -> Vec<(Recipe, bool)> {
    session
        .tabs()
        .filter(|tab| tab.pinned)
        .map(|tab| (tab.recipe.clone(), tab.tab.kind() == ViewerTabKind::Live))
        .collect()
}

fn persist(connection: &mut Connection, records: &[(Recipe, bool)]) -> Result<(), PinnedTabsError> {
    let transaction = connection.transaction().map_err(anyhow::Error::from)?;
    persist_records(&transaction, records)?;
    transaction.commit().map_err(anyhow::Error::from)?;
    Ok(())
}

pub(super) fn persist_records(
    transaction: &rusqlite::Transaction<'_>,
    records: &[(Recipe, bool)],
) -> Result<(), PinnedTabsError> {
    transaction
        .execute("DELETE FROM pinned_viewer_tabs", [])
        .map_err(anyhow::Error::from)?;
    for (position, (recipe, live)) in records.iter().enumerate() {
        let position = i64::try_from(position).map_err(anyhow::Error::from)?;
        let json = serde_json::to_string(recipe).map_err(anyhow::Error::from)?;
        transaction
            .execute(
                "INSERT INTO pinned_viewer_tabs (position, recipe_json, live) VALUES (?1, ?2, ?3)",
                params![position, json, live],
            )
            .map_err(anyhow::Error::from)?;
    }
    Ok(())
}

pub fn restore(
    state: &ViewerState,
    connection: &Connection,
) -> Result<Vec<ReservedRecipeWork>, PinnedTabsError> {
    let mut statement = connection
        .prepare("SELECT recipe_json, live FROM pinned_viewer_tabs ORDER BY position")
        .map_err(anyhow::Error::from)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
        })
        .map_err(anyhow::Error::from)?;
    let mut work = Vec::new();
    for row in rows {
        let (json, live) = row.map_err(anyhow::Error::from)?;
        let recipe = serde_json::from_str(&json).map_err(anyhow::Error::from)?;
        let reserved = work::reserve_restored_pin(
            state,
            recipe,
            if live {
                ViewerTabKind::Live
            } else {
                ViewerTabKind::Snapshot
            },
        )
        .map_err(anyhow::Error::from)?;
        work.push(reserved);
    }
    Ok(work)
}

#[cfg(test)]
mod tests {
    use gtl_models::recipes::RecipeBatchId;

    use super::*;
    use crate::{
        recipes::{RecipeOp, RecipeTarget},
        utils,
        viewer::{
            close_viewer_tabs::{self, CloseViewerTabs},
            pinned_tabs,
            session::CachedView,
        },
    };

    #[test]
    fn pins_restore_and_failed_storage_leaves_the_session_unchanged() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE pinned_viewer_tabs (position INTEGER PRIMARY KEY, recipe_json TEXT NOT NULL, live INTEGER NOT NULL) STRICT").unwrap();
        let state = ViewerState::new();
        let recipe = utils::viewer::recipe(RecipeOp::Diff {
            target: RecipeTarget::Unpushed {
                pinned: Some(utils::pinned_range(&"a".repeat(40), &"b".repeat(40))),
            },
        });
        let work = work::reserve_open(
            &state,
            recipe.clone(),
            RecipeBatchId::generate(),
            ViewerTabKind::Snapshot,
        )
        .unwrap();
        let tab_id = work.ticket().tab_id;
        state
            .update(|session| {
                session.publish_labeled_if_current(
                    work.ticket(),
                    CachedView::new(std::sync::Arc::new(utils::viewer::empty_view())),
                    "snapshot".into(),
                )
            })
            .unwrap();
        pinned_tabs::execute(
            SetViewerTabPinned {
                tab_id,
                pinned: true,
            },
            &state,
            &mut connection,
        )
        .unwrap();
        let restored = ViewerState::new();
        let restored_work = restore(&restored, &connection).unwrap();
        assert_eq!(restored_work.len(), 1);
        assert_eq!(
            restored
                .inspect(|session| session.focus_request_version())
                .unwrap(),
            None
        );
        restored
            .inspect(|session| {
                let tab = session.tabs().next().unwrap();
                assert!(tab.pinned);
                assert_eq!(tab.recipe, recipe);
            })
            .unwrap();
        for request in [
            CloseViewerTabs::Others(tab_id),
            CloseViewerTabs::One(tab_id),
        ] {
            close_viewer_tabs::execute(request, &mut connection, &state).unwrap();
        }
        assert_eq!(state.inspect(|session| session.tabs().len()).unwrap(), 1);
        connection
            .execute_batch("DROP TABLE pinned_viewer_tabs")
            .unwrap();
        assert!(
            pinned_tabs::execute(
                SetViewerTabPinned {
                    tab_id,
                    pinned: false
                },
                &state,
                &mut connection
            )
            .is_err()
        );
        assert!(
            state
                .inspect(|session| session.tab(tab_id).unwrap().pinned)
                .unwrap()
        );
    }
}
