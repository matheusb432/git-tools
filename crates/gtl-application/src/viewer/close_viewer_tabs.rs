use gtl_models::viewer::ViewerTabId;
use rusqlite::{Connection, params};

use crate::viewer::{
    ViewerState, ViewerStateError,
    work::{self, ReserveRecipeError, ReservedRecipeWork},
};

#[derive(Clone, Copy)]
pub enum CloseViewerTabs {
    One(ViewerTabId),
    Others(ViewerTabId),
}

#[derive(Debug, thiserror::Error)]
pub enum CloseViewerTabsError {
    #[error("viewer tab is not available")]
    UnknownTab,
    #[error(transparent)]
    ViewerState(#[from] ViewerStateError),
    #[error(transparent)]
    ReserveWork(#[from] ReserveRecipeError),
    #[error(transparent)]
    Persistence(#[from] rusqlite::Error),
}

#[cqrsy::command]
pub fn execute(
    request: CloseViewerTabs,
    connection: &mut Connection,
    viewer_state: &ViewerState,
) -> Result<Option<ReservedRecipeWork>, CloseViewerTabsError> {
    viewer_state.update(|session| {
        let (CloseViewerTabs::One(tab_id) | CloseViewerTabs::Others(tab_id)) = request;
        if session.tab(tab_id).is_none() {
            return Err(CloseViewerTabsError::UnknownTab);
        }
        let ids = session.tabs().filter(|tab| {
            !tab.pinned && match request {
                CloseViewerTabs::One(id) => tab.tab.id() == id,
                CloseViewerTabs::Others(id) => tab.tab.id() != id,
            }
        }).map(|tab| tab.tab.id()).collect::<Vec<_>>();
        let sources = ids.iter().filter_map(|id| session.live_source(*id)).collect::<Vec<_>>();
        if !sources.is_empty() {
            let transaction = connection.transaction()?;
            for (source, comparison) in sources {
                transaction.execute(
                    "DELETE FROM live_views WHERE source_kind = ?1 AND source_value = ?2 AND comparison = ?3",
                    params![source.kind(), source.value(), comparison.as_str()],
                )?;
            }
            transaction.commit()?;
        }
        let active = session.active();
        for id in ids {
            session.close(id);
        }
        if session.active() == active {
            Ok(None)
        } else {
            work::reserve_active_if_needed(session).map_err(Into::into)
        }
    })?
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::{ViewerTabId, ViewerTabKind};
    use rusqlite::Connection;

    use super::{CloseViewerTabs, CloseViewerTabsError};
    use crate::{
        live_views::list_live_views,
        recipes::{RecipeOp, RecipeTarget},
        utils,
        viewer::{ViewerState, close_viewer_tabs, work},
    };

    fn store_test() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE live_views (
            id INTEGER PRIMARY KEY, source_kind TEXT NOT NULL, source_value TEXT NOT NULL, comparison TEXT NOT NULL DEFAULT 'unpushed_commits',
            display_name TEXT NOT NULL, created_at TEXT NOT NULL, last_opened_at TEXT
        )").unwrap();
        connection
    }

    fn seed_live_view(connection: &Connection) {
        connection
            .execute(
                "INSERT INTO live_views \
                 (source_kind, source_value, display_name, created_at) \
                 VALUES ('LocalRepo', '/repos/project', 'project', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
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
        .unwrap()
        .ticket()
        .tab_id
    }

    #[test]
    fn close_removes_the_saved_view_and_its_tab_together() {
        let mut connection = store_test();
        seed_live_view(&connection);
        let viewer = ViewerState::new();
        let tab_id = open_live_tab(&viewer);

        let refresh =
            close_viewer_tabs::execute(CloseViewerTabs::One(tab_id), &mut connection, &viewer)
                .unwrap();

        assert!(refresh.is_none());
        assert!(
            list_live_views::execute(list_live_views::ListLiveViews, &connection)
                .unwrap()
                .is_empty()
        );
        assert!(
            viewer
                .inspect(|session| session.tab(tab_id).is_none())
                .unwrap()
        );
    }

    #[test]
    fn closing_a_live_tab_preserves_other_comparisons_and_pinned_tabs() {
        let mut connection = store_test();
        seed_live_view(&connection);
        connection.execute("INSERT INTO live_views (source_kind, source_value, comparison, display_name, created_at) VALUES ('LocalRepo', '/repos/project', 'local_changes', 'project', '2026-01-01T00:00:00Z')", []).unwrap();
        let viewer = ViewerState::new();
        let unpushed = open_live_tab(&viewer);
        let local = work::reserve_open(
            &viewer,
            utils::viewer::recipe(RecipeOp::Diff {
                target: RecipeTarget::Base {
                    rev: gtl_models::git::GitRevision::head(),
                },
            }),
            gtl_models::recipes::RecipeBatchId::generate(),
            ViewerTabKind::Live,
        )
        .unwrap()
        .ticket()
        .tab_id;
        viewer
            .update(|session| session.set_pinned(unpushed, true))
            .unwrap();
        close_viewer_tabs::execute(CloseViewerTabs::Others(unpushed), &mut connection, &viewer)
            .unwrap();
        let saved = list_live_views::execute(list_live_views::ListLiveViews, &connection).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(
            saved[0].comparison,
            gtl_models::live_views::LiveComparison::UnpushedCommits
        );
        assert!(
            viewer
                .inspect(|session| session.tab(local).is_none())
                .unwrap()
        );
        close_viewer_tabs::execute(CloseViewerTabs::One(unpushed), &mut connection, &viewer)
            .unwrap();
        assert!(
            viewer
                .inspect(|session| session.tab(unpushed).is_some())
                .unwrap()
        );
        assert_eq!(
            list_live_views::execute(list_live_views::ListLiveViews, &connection)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn closing_snapshots_refreshes_only_when_the_active_tab_changes() {
        let mut connection = store_test();
        let viewer = ViewerState::new();
        let first = work::reserve_open(
            &viewer,
            utils::viewer::recipe(RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            }),
            gtl_models::recipes::RecipeBatchId::generate(),
            ViewerTabKind::Snapshot,
        )
        .unwrap()
        .ticket()
        .tab_id;
        let second = open_live_tab(&viewer);
        let refresh =
            close_viewer_tabs::execute(CloseViewerTabs::One(second), &mut connection, &viewer)
                .unwrap()
                .unwrap();
        assert_eq!(refresh.ticket().tab_id, first);
        let second = open_live_tab(&viewer);
        assert!(
            close_viewer_tabs::execute(CloseViewerTabs::One(first), &mut connection, &viewer)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            viewer.inspect(|session| session.active()).unwrap(),
            Some(second)
        );
    }

    #[test]
    fn persistence_failure_leaves_the_live_tab_open() {
        let mut connection = store_test();
        connection.execute("DROP TABLE live_views", []).unwrap();
        let viewer = ViewerState::new();
        let tab_id = open_live_tab(&viewer);

        let error =
            close_viewer_tabs::execute(CloseViewerTabs::One(tab_id), &mut connection, &viewer)
                .unwrap_err();

        assert!(matches!(error, CloseViewerTabsError::Persistence(_)));
        assert!(
            viewer
                .inspect(|session| session.tab(tab_id).is_some())
                .unwrap()
        );
    }
}
