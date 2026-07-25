//! The `history/get_recent_render` vertical slice: reopen one persisted render
//! recipe by its stable row identity.

use domain::viewer::RenderHistoryId;
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    history::{
        RecentRenderRecord, RecentRenderRowError,
        persistence::{RECENT_RENDER_SELECT, RecentRenderRow},
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
    #[error(transparent)]
    InvalidRow(#[from] RecentRenderRowError),
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
        .prepare_cached(&format!("{RECENT_RENDER_SELECT} WHERE r.id = ?1"))
        .map_err(anyhow::Error::from)?;
    let row = statement
        .query_row(params![i64::from(id)], RecentRenderRow::from_row)
        .optional()
        .map_err(anyhow::Error::from)?;
    row.map(|row| Ok(row.try_into_record()?)).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::persistence::{seed_recent_render, store_test};

    #[test]
    fn recent_render_is_looked_up_by_stable_id() {
        let id = RenderHistoryId::try_new(11).expect("positive id");
        let store = store_test();
        seed_recent_render(&store, i64::from(id), "render");

        let response = execute(GetRecentRender { id }, &store).expect("lookup succeeds");

        assert_eq!(response.entry.expect("record exists").id, id);
    }

    #[test]
    fn absent_recent_render_is_a_successful_miss() {
        let id = RenderHistoryId::try_new(99).expect("positive id");

        let store = store_test();
        let response = execute(GetRecentRender { id }, &store).expect("lookup succeeds");

        assert!(response.entry.is_none());
    }
}
