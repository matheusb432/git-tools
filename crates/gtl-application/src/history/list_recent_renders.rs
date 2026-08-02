//! The `history/list_recent_renders` vertical slice: every persisted render
//! recipe, newest first.

use rusqlite::Connection;

use crate::history::{
    RecentRenderRecord, RecentRenderRowError,
    persistence::{RECENT_RENDER_SELECT, RecentRenderRow},
};

/// Requests every recent render in the store's newest-first order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRecentRenders;

/// Returns recent persisted recipes with their stable database identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRecentRendersOk {
    pub entries: Vec<RecentRenderRecord>,
}

/// Reports a recent-render listing failure.
#[derive(Debug, thiserror::Error)]
pub enum ListRecentRendersError {
    #[error(transparent)]
    InvalidRow(#[from] RecentRenderRowError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Lists recent renders from the application database connection.
pub fn execute(
    _query: ListRecentRenders,
    connection: &Connection,
) -> Result<ListRecentRendersOk, ListRecentRendersError> {
    Ok(ListRecentRendersOk {
        entries: list_recent_renders(connection)?,
    })
}

fn list_recent_renders(
    connection: &Connection,
) -> Result<Vec<RecentRenderRecord>, ListRecentRendersError> {
    let mut statement = connection
        .prepare_cached(&format!("{RECENT_RENDER_SELECT} ORDER BY r.id DESC"))
        .map_err(anyhow::Error::from)?;
    let rows = statement
        .query_map([], RecentRenderRow::from_row)
        .map_err(anyhow::Error::from)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(anyhow::Error::from)?;
    rows.into_iter()
        .map(|row| Ok(row.try_into_record()?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{
        RecentRenderRowError,
        persistence::{seed_recent_render, store_test},
    };

    #[test]
    fn recent_history_exposes_stable_database_ids_newest_first() {
        let connection = store_test();
        seed_recent_render(&connection, 10, "older");
        seed_recent_render(&connection, 11, "newer");

        let response = execute(ListRecentRenders, &connection).expect("list succeeds");

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
    fn recent_history_maps_invalid_persisted_id_to_the_list_operation_error() {
        let connection = store_test();
        seed_recent_render(&connection, 0, "invalid");

        let error =
            execute(ListRecentRenders, &connection).expect_err("corrupt row identity rejects");

        assert!(matches!(
            error,
            ListRecentRendersError::InvalidRow(RecentRenderRowError::InvalidId { id: 0 })
        ));
    }

    #[test]
    fn recent_history_maps_an_unknown_operation_to_the_recipe_decode_error() {
        let connection = store_test();
        connection
            .execute_batch(
                "INSERT INTO project_sources (id, kind, value, created_at) \
                 VALUES (7, 'directory', '/repos/gt', '2026-07-11T00:00:00Z');
                 INSERT INTO render_operations (id, name) VALUES (9, 'bogus');
                 INSERT INTO recent_renders \
                 (id, source_id, operation_id, title, repo_name, range_label, rendered_at) \
                 VALUES (1, 7, 9, 't', 'git-tools', 'main..HEAD', '2026-07-11T00:00:00Z');",
            )
            .expect("seed undecodable render");

        let error = execute(ListRecentRenders, &connection).expect_err("unknown operation rejects");

        assert!(matches!(
            error,
            ListRecentRendersError::InvalidRow(RecentRenderRowError::InvalidRecipe { id: 1, .. })
        ));
    }
}
