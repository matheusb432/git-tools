//! The recent-render history queries used to list and reopen persisted recipes.

pub mod list {
    use rusqlite::Connection;

    use crate::{
        history::{
            RecentRenderRecord,
            persistence::{RecentRenderIdentityError, RecentRenderRow},
        },
        ports::AppStateStore,
    };

    /// Requests every recent render in the store's newest-first order.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ListRecentRenders;

    /// Returns recent persisted recipes with their stable database identities.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ListRecentRendersResponse {
        pub entries: Vec<RecentRenderRecord>,
    }

    /// Reports a recent-render listing failure.
    #[derive(Debug, thiserror::Error)]
    pub enum ListRecentRendersError {
        #[error("recent_renders row id {id} violates the positive-ID invariant")]
        InvalidRecentRenderId { id: i64 },
        #[error(transparent)]
        Unexpected(#[from] anyhow::Error),
    }

    /// Lists recent renders through the app-state persistence port.
    #[cqrsy::query]
    pub fn execute(
        _query: ListRecentRenders,
        store: &impl AppStateStore,
    ) -> Result<ListRecentRendersResponse, ListRecentRendersError> {
        let connection = store.connection_lock()?;
        Ok(ListRecentRendersResponse {
            entries: list_recent_renders(&connection)?,
        })
    }

    fn list_recent_renders(
        connection: &Connection,
    ) -> Result<Vec<RecentRenderRecord>, ListRecentRendersError> {
        let mut statement = connection
            .prepare_cached(
                "SELECT id, recipe_json, title, repo_name, kind, range_label, rendered_at
                 FROM recent_renders ORDER BY id DESC",
            )
            .map_err(anyhow::Error::from)?;
        let rows = statement
            .query_map([], RecentRenderRow::from_row)
            .map_err(anyhow::Error::from)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(anyhow::Error::from)?;
        rows.into_iter().map(recent_render_from_row).collect()
    }

    fn recent_render_from_row(
        row: RecentRenderRow,
    ) -> Result<RecentRenderRecord, ListRecentRendersError> {
        row.try_into_record()
            .map_err(|RecentRenderIdentityError { id }| {
                ListRecentRendersError::InvalidRecentRenderId { id }
            })
    }
}

pub mod get {
    use domain::viewer::RenderHistoryId;
    use rusqlite::{Connection, OptionalExtension, params};

    use crate::{
        history::{
            RecentRenderRecord,
            persistence::{RecentRenderIdentityError, RecentRenderRow},
        },
        ports::AppStateStore,
    };

    /// Requests one recent render by its stable persisted-row identity.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GetRecentRender {
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
        #[error("recent_renders row id {id} violates the positive-ID invariant")]
        InvalidRecentRenderId { id: i64 },
        #[error(transparent)]
        Unexpected(#[from] anyhow::Error),
    }

    /// Gets a recent render through the app-state persistence port.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "cqrsy requires request-first operations to take requests by value"
    )]
    #[cqrsy::query]
    pub fn execute(
        query: GetRecentRender,
        store: &impl AppStateStore,
    ) -> Result<GetRecentRenderResponse, GetRecentRenderError> {
        let GetRecentRender { id } = query;
        let connection = store.connection_lock()?;
        Ok(GetRecentRenderResponse {
            entry: get_recent_render(&connection, id)?,
        })
    }

    fn get_recent_render(
        connection: &Connection,
        id: RenderHistoryId,
    ) -> Result<Option<RecentRenderRecord>, GetRecentRenderError> {
        let mut statement = connection
            .prepare_cached(
                "SELECT id, recipe_json, title, repo_name, kind, range_label, rendered_at
                 FROM recent_renders WHERE id = ?1",
            )
            .map_err(anyhow::Error::from)?;
        let row = statement
            .query_row(params![i64::from(id)], RecentRenderRow::from_row)
            .optional()
            .map_err(anyhow::Error::from)?;
        row.map(recent_render_from_row).transpose()
    }

    fn recent_render_from_row(
        row: RecentRenderRow,
    ) -> Result<RecentRenderRecord, GetRecentRenderError> {
        row.try_into_record()
            .map_err(|RecentRenderIdentityError { id }| {
                GetRecentRenderError::InvalidRecentRenderId { id }
            })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::history::persistence::RecentRenderRow;

        #[test]
        fn invalid_persisted_id_maps_to_the_get_operation_error() {
            let connection = rusqlite::Connection::open_in_memory().expect("in-memory connection");
            let row = connection
                .query_row(
                    "SELECT 0, '{}', 'invalid', 'repo', 'diff', \
                     'main..HEAD', '2026-01-01T00:00:00Z'",
                    [],
                    RecentRenderRow::from_row,
                )
                .expect("decode corrupt row");

            let error =
                recent_render_from_row(row).expect_err("invalid persisted identity rejects");

            assert!(matches!(
                error,
                GetRecentRenderError::InvalidRecentRenderId { id: 0 }
            ));
        }
    }
}

pub use get::{GetRecentRender, GetRecentRenderError, GetRecentRenderResponse};
pub use list::{ListRecentRenders, ListRecentRendersError, ListRecentRendersResponse};

#[cfg(test)]
mod tests {
    use domain::viewer::RenderHistoryId;

    use super::{GetRecentRender, ListRecentRenders, ListRecentRendersError, get, list};
    use crate::{history::persistence::store_test, ports::AppStateStore};

    fn seed_recent(store: &impl AppStateStore, id: i64, title: &str) {
        store
            .connection_lock()
            .expect("connection lock")
            .execute(
                "INSERT INTO recent_renders \
                 (id, recipe_json, title, repo_name, kind, range_label, rendered_at) \
                 VALUES (?1, ?2, ?3, 'git-tools', 'diff', 'main..HEAD', \
                 '2026-07-11T00:00:00Z')",
                rusqlite::params![id, format!(r#"{{"title":"{title}"}}"#), title],
            )
            .expect("seed recent render");
    }

    #[test]
    fn recent_history_exposes_stable_database_ids_newest_first() {
        let store = store_test();
        seed_recent(&store, 10, "older");
        seed_recent(&store, 11, "newer");

        let response = list::execute(ListRecentRenders, &store).expect("list succeeds");

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
        let store = store_test();
        seed_recent(&store, i64::from(id), "render");

        let response = get::execute(GetRecentRender { id }, &store).expect("lookup succeeds");

        assert_eq!(response.entry.expect("record exists").id, id);
    }

    #[test]
    fn absent_recent_render_is_a_successful_miss() {
        let id = RenderHistoryId::try_new(99).expect("positive id");

        let store = store_test();
        let response = get::execute(GetRecentRender { id }, &store).expect("lookup succeeds");

        assert!(response.entry.is_none());
    }

    #[test]
    fn recent_history_maps_invalid_persisted_id_to_the_list_operation_error() {
        let store = store_test();
        seed_recent(&store, 0, "invalid");

        let error =
            list::execute(ListRecentRenders, &store).expect_err("corrupt row identity rejects");

        assert!(matches!(
            error,
            ListRecentRendersError::InvalidRecentRenderId { id: 0 }
        ));
    }
}
