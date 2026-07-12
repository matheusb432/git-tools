//! The recent-render history queries used to list and reopen persisted recipes.

mod list {
    use std::path::PathBuf;

    use crate::ports::{AppStateError, AppStateStore, RecentRenderRecord};

    /// Requests every recent render in the store's newest-first order.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ListRecentRenders {
        pub data_root: PathBuf,
    }

    /// Returns recent persisted recipes with their stable database identities.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ListRecentRendersResponse {
        pub entries: Vec<RecentRenderRecord>,
    }

    /// Reports a typed app-state failure while listing recent renders.
    #[derive(Debug, thiserror::Error)]
    pub enum ListRecentRendersError {
        #[error(transparent)]
        AppState(#[from] AppStateError),
    }

    /// Lists recent renders through the app-state persistence port.
    #[cqrsy::handler(query)]
    pub fn handle(
        store: &impl AppStateStore,
        query: ListRecentRenders,
    ) -> Result<ListRecentRendersResponse, ListRecentRendersError> {
        Ok(ListRecentRendersResponse {
            entries: store.list_recent_renders(&query.data_root)?,
        })
    }
}

mod get {
    use std::path::PathBuf;

    use domain::viewer::RenderHistoryId;

    use crate::ports::{AppStateError, AppStateStore, RecentRenderRecord};

    /// Requests one recent render by its stable persisted-row identity.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GetRecentRender {
        pub data_root: PathBuf,
        pub id: RenderHistoryId,
    }

    /// Returns the matching render or a successful miss when it no longer exists.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GetRecentRenderResponse {
        pub entry: Option<RecentRenderRecord>,
    }

    /// Reports an unexpected stable-ID recent-render lookup failure.
    #[derive(Debug, thiserror::Error)]
    pub enum GetRecentRenderError {
        #[error(transparent)]
        AppState(#[from] AppStateError),
    }

    /// Gets a recent render through the app-state persistence port.
    #[cqrsy::handler(query)]
    pub fn handle(
        store: &impl AppStateStore,
        query: GetRecentRender,
    ) -> Result<GetRecentRenderResponse, GetRecentRenderError> {
        Ok(GetRecentRenderResponse {
            entry: store.get_recent_render(&query.data_root, query.id)?,
        })
    }
}

pub use get::{
    GetRecentRender, GetRecentRenderError, GetRecentRenderHandler, GetRecentRenderResponse,
};
pub use list::{
    ListRecentRenders, ListRecentRendersError, ListRecentRendersHandler, ListRecentRendersResponse,
};

#[cfg(test)]
mod tests {
    use cqrsy::Sender;
    use domain::viewer::RenderHistoryId;

    use super::{
        GetRecentRender, GetRecentRenderHandler, ListRecentRenders, ListRecentRendersError,
        ListRecentRendersHandler,
    };
    use crate::{
        ports::{AppStateError, RecentRenderRecord},
        testing::InMemoryAppStateStore,
    };

    fn recent(id: RenderHistoryId, title: &str) -> RecentRenderRecord {
        RecentRenderRecord {
            id,
            recipe_json: format!(r#"{{"title":"{title}"}}"#),
            title: title.into(),
            repo_name: "git-tools".into(),
            kind: "diff".into(),
            range_label: "main..HEAD".into(),
            rendered_at: "2026-07-11T00:00:00Z".into(),
        }
    }

    #[test]
    fn recent_history_exposes_stable_database_ids_newest_first() {
        let store = InMemoryAppStateStore::default();
        store.renders.lock().expect("renders lock").extend([
            recent(RenderHistoryId::try_new(10).expect("positive id"), "older"),
            recent(RenderHistoryId::try_new(11).expect("positive id"), "newer"),
        ]);

        let response = ListRecentRendersHandler { store }
            .send_now(ListRecentRenders {
                data_root: "/data".into(),
            })
            .expect("list succeeds");

        assert_eq!(
            response
                .entries
                .iter()
                .map(|entry| i64::from(entry.id))
                .collect::<Vec<_>>(),
            vec![11, 10]
        );
    }

    #[test]
    fn recent_render_is_looked_up_by_stable_id() {
        let id = RenderHistoryId::try_new(11).expect("positive id");
        let store = InMemoryAppStateStore::default();
        store
            .renders
            .lock()
            .expect("renders lock")
            .push(recent(id, "render"));

        let response = GetRecentRenderHandler { store }
            .send_now(GetRecentRender {
                data_root: "/data".into(),
                id,
            })
            .expect("lookup succeeds");

        assert_eq!(response.entry.expect("record exists").id, id);
    }

    #[test]
    fn absent_recent_render_is_a_successful_miss() {
        let id = RenderHistoryId::try_new(99).expect("positive id");

        let response = GetRecentRenderHandler {
            store: InMemoryAppStateStore::default(),
        }
        .send_now(GetRecentRender {
            data_root: "/data".into(),
            id,
        })
        .expect("lookup succeeds");

        assert!(response.entry.is_none());
    }

    #[test]
    fn recent_history_preserves_typed_store_corruption() {
        let store = InMemoryAppStateStore::default();
        *store.list_error_id.lock().expect("list error lock") = Some(0);

        let error = ListRecentRendersHandler { store }
            .send_now(ListRecentRenders {
                data_root: "/data".into(),
            })
            .expect_err("corrupt row identity rejects");

        assert!(matches!(
            error,
            ListRecentRendersError::AppState(AppStateError::InvalidRecentRenderId { id: 0 })
        ));
    }
}
