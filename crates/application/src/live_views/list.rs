//! The `live_views/list` vertical slice: every saved live view, creation order.

use rusqlite::Connection;

use crate::{live_views::LiveViewRecord, ports::AppStateStore};

/// Lists every saved live view.
#[derive(Debug, Clone, PartialEq)]
pub struct ListLiveViews;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListLiveViewsOk {
    pub views: Vec<LiveViewRecord>,
}

#[derive(Debug, thiserror::Error)]
pub enum ListLiveViewsError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Lists saved live views through the app-state port.
#[cqrsy::query]
pub fn execute(
    _query: ListLiveViews,
    store: &impl AppStateStore,
) -> Result<ListLiveViewsOk, ListLiveViewsError> {
    let connection = store.connection_lock()?;
    let views = list_live_views(&connection)?;
    Ok(ListLiveViewsOk { views })
}

fn list_live_views(connection: &Connection) -> anyhow::Result<Vec<LiveViewRecord>> {
    let mut statement = connection.prepare_cached(
        "SELECT source_kind, source_value, display_name, created_at, last_opened_at
         FROM live_views ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(LiveViewRecord {
            source_kind: row.get(0)?,
            source_value: row.get(1)?,
            display_name: row.get(2)?,
            created_at: row.get(3)?,
            last_opened_at: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{live_views::persistence::store_test, ports::AppStateStore};

    #[test]
    fn lists_saved_views_in_creation_order() {
        let store = store_test();
        let connection = store.connection_lock().expect("connection lock");
        connection
            .execute(
                "INSERT INTO live_views \
                 (id, source_kind, source_value, display_name, created_at) VALUES \
                 (2, 'LocalRepo', '/repos/b', 'b', '2026-01-02T00:00:00Z'), \
                 (1, 'LocalRepo', '/repos/a', 'a', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("seed live views");
        drop(connection);
        let response = execute(ListLiveViews, &store).expect("list succeeds");

        assert_eq!(
            response
                .views
                .iter()
                .map(|v| v.source_value.as_str())
                .collect::<Vec<_>>(),
            vec!["/repos/a", "/repos/b"]
        );
    }
}
