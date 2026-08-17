//! The `live_views/remove` vertical slice: delete a saved live view by identity.

use gtl_models::live_views::LiveSource;
use rusqlite::{Connection, params};

/// Delete the saved live view identified by its source.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoveLiveView {
    pub source: LiveSource,
}

/// Whether the requested source was present in the saved collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveLiveViewOk {
    Removed,
    NotFound,
}

#[derive(Debug, thiserror::Error)]
pub enum RemoveLiveViewError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Removes a saved live view from the application database connection.
pub fn execute(
    req: RemoveLiveView,
    connection: &Connection,
) -> Result<RemoveLiveViewOk, RemoveLiveViewError> {
    let RemoveLiveView { source } = req;
    Ok(remove_live_view(connection, &source)?)
}

fn remove_live_view(
    connection: &Connection,
    source: &LiveSource,
) -> anyhow::Result<RemoveLiveViewOk> {
    let mut statement = connection
        .prepare_cached("DELETE FROM live_views WHERE source_kind = ?1 AND source_value = ?2")?;
    let source_value = source.value();
    Ok(
        match statement.execute(params![source.kind(), source_value])? {
            0 => RemoveLiveViewOk::NotFound,
            _ => RemoveLiveViewOk::Removed,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_views::{persistence::store_test, remove_live_view};

    fn seed_live_view(connection: &Connection) {
        connection
            .execute(
                "INSERT INTO live_views \
                 (source_kind, source_value, display_name, created_at) \
                 VALUES ('LocalRepo', '/repos/gt', 'gt', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("seed live view");
    }

    #[test]
    fn remove_reports_removed_when_a_row_was_present() {
        let connection = store_test();
        seed_live_view(&connection);
        let response = remove_live_view::execute(
            RemoveLiveView {
                source: LiveSource::local_repo(crate::utils::repository_root("/repos/gt")),
            },
            &connection,
        )
        .expect("remove succeeds");

        assert_eq!(response, RemoveLiveViewOk::Removed);
    }

    #[test]
    fn remove_reports_not_found_for_an_unknown_identity() {
        let connection = store_test();
        let response = remove_live_view::execute(
            RemoveLiveView {
                source: LiveSource::local_repo(crate::utils::repository_root("/repos/unknown")),
            },
            &connection,
        )
        .expect("remove succeeds");

        assert_eq!(response, RemoveLiveViewOk::NotFound);
    }

    #[test]
    fn delete_failure_is_an_unexpected_operation_error() {
        let connection = store_test();
        connection
            .execute("DROP TABLE live_views", [])
            .expect("drop live views table");

        let error = remove_live_view::execute(
            RemoveLiveView {
                source: LiveSource::local_repo(crate::utils::repository_root("/repos/gt")),
            },
            &connection,
        )
        .expect_err("delete failure rejects");

        assert!(matches!(error, RemoveLiveViewError::Unexpected(_)));
    }
}
