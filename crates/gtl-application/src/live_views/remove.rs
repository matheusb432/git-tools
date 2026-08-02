//! The `live_views/remove` vertical slice: delete a saved live view by identity.

use rusqlite::{Connection, params};

/// Delete the saved live view identified by `(source_kind, source_value)`.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoveLiveView {
    pub source_kind: String,
    pub source_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveLiveViewOk {
    pub removed: bool,
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
    let RemoveLiveView {
        source_kind,
        source_value,
    } = req;
    let removed = remove_live_view(connection, &source_kind, &source_value)?;
    Ok(RemoveLiveViewOk { removed })
}

fn remove_live_view(
    connection: &Connection,
    source_kind: &str,
    source_value: &str,
) -> anyhow::Result<bool> {
    let mut statement = connection
        .prepare_cached("DELETE FROM live_views WHERE source_kind = ?1 AND source_value = ?2")?;
    Ok(statement.execute(params![source_kind, source_value])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_views::persistence::store_test;

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
    fn remove_reports_true_when_a_row_was_removed() {
        let connection = store_test();
        seed_live_view(&connection);
        let response = execute(
            RemoveLiveView {
                source_kind: "LocalRepo".into(),
                source_value: "/repos/gt".into(),
            },
            &connection,
        )
        .expect("remove succeeds");

        assert!(response.removed);
    }

    #[test]
    fn remove_reports_false_for_unknown_identity() {
        let connection = store_test();
        let response = execute(
            RemoveLiveView {
                source_kind: "LocalRepo".into(),
                source_value: "/repos/unknown".into(),
            },
            &connection,
        )
        .expect("remove succeeds");

        assert!(!response.removed);
    }

    #[test]
    fn delete_failure_is_an_unexpected_operation_error() {
        let connection = store_test();
        connection
            .execute("DROP TABLE live_views", [])
            .expect("drop live views table");

        let error = execute(
            RemoveLiveView {
                source_kind: "LocalRepo".into(),
                source_value: "/repos/gt".into(),
            },
            &connection,
        )
        .expect_err("delete failure rejects");

        assert!(matches!(error, RemoveLiveViewError::Unexpected(_)));
    }
}
