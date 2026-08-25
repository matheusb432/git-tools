//! The `live_views/list` vertical slice: every saved live view, creation order.

use gtl_models::{
    live_views::{LiveSource, ParseLiveSourceError},
    paths::ProjectNameError,
    timestamps::MachineTimestamp,
};
use rusqlite::Connection;

use crate::live_views::LiveViewRecord;

/// Lists every saved live view.
#[derive(Debug, Clone, PartialEq)]
pub struct ListLiveViews;

#[derive(Debug, thiserror::Error)]
pub enum ListLiveViewsError {
    #[error(transparent)]
    InvalidSource(#[from] ParseLiveSourceError),
    #[error(transparent)]
    InvalidDisplayName(#[from] ProjectNameError),
    #[error("live view `{source_value}` has invalid {field}: {reason}")]
    InvalidTimestamp {
        source_value: String,
        field: &'static str,
        reason: String,
    },
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Lists saved live views from the application database connection.
pub fn execute(
    _query: ListLiveViews,
    connection: &Connection,
) -> Result<Vec<LiveViewRecord>, ListLiveViewsError> {
    list_live_views(connection)
}

fn list_live_views(connection: &Connection) -> Result<Vec<LiveViewRecord>, ListLiveViewsError> {
    let mut statement = connection
        .prepare_cached(
            "SELECT source_kind, source_value, display_name, created_at, last_opened_at
             FROM live_views ORDER BY id",
        )
        .map_err(anyhow::Error::from)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(anyhow::Error::from)?;
    rows.map(|row| {
        let (source_kind, source_value, display_name, created_at, last_opened_at) =
            row.map_err(anyhow::Error::from)?;
        let created_at = MachineTimestamp::try_from(created_at).map_err(|error| {
            ListLiveViewsError::InvalidTimestamp {
                source_value: source_value.clone(),
                field: "created_at",
                reason: error.to_string(),
            }
        })?;
        let last_opened_at = last_opened_at
            .map(MachineTimestamp::try_from)
            .transpose()
            .map_err(|error| ListLiveViewsError::InvalidTimestamp {
                source_value: source_value.clone(),
                field: "last_opened_at",
                reason: error.to_string(),
            })?;
        Ok(LiveViewRecord {
            source: LiveSource::from_parts(&source_kind, &source_value)?,
            display_name: display_name.try_into()?,
            created_at,
            last_opened_at,
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_views::{list_live_views, persistence::store_test};

    #[test]
    fn lists_saved_views_in_creation_order() {
        let connection = store_test();
        connection
            .execute(
                "INSERT INTO live_views \
                 (id, source_kind, source_value, display_name, created_at) VALUES \
                 (2, 'LocalRepo', '/repos/b', 'b', '2026-01-02T00:00:00Z'), \
                 (1, 'LocalRepo', '/repos/a', 'a', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("seed live views");
        let response = list_live_views::execute(ListLiveViews, &connection).expect("list succeeds");

        assert_eq!(
            response
                .iter()
                .map(|view| view.source.value())
                .collect::<Vec<_>>(),
            vec!["/repos/a".to_owned(), "/repos/b".to_owned()]
        );
    }

    #[test]
    fn unsupported_persisted_source_kinds_fail_at_the_decode_boundary() {
        let connection = store_test();
        connection
            .execute(
                "INSERT INTO live_views \
                 (source_kind, source_value, display_name, created_at) VALUES \
                 ('RemoteRepo', 'owner/repo', 'repo', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("seed unsupported live view");

        let error =
            list_live_views::execute(ListLiveViews, &connection).expect_err("unknown kind rejects");

        assert!(matches!(
            error,
            ListLiveViewsError::InvalidSource(ParseLiveSourceError::UnknownKind { kind })
                if kind == "RemoteRepo"
        ));
    }

    #[test]
    fn timezone_less_persisted_timestamp_fails_at_the_decode_boundary() {
        let connection = store_test();
        connection
            .execute(
                "INSERT INTO live_views \
                 (source_kind, source_value, display_name, created_at) VALUES \
                 ('LocalRepo', '/repos/gt', 'gt', '2026-01-01T00:00:00')",
                [],
            )
            .expect("seed malformed live view");

        let error = list_live_views::execute(ListLiveViews, &connection)
            .expect_err("timezone-less timestamp must reject");

        assert!(matches!(
            error,
            ListLiveViewsError::InvalidTimestamp {
                source_value,
                field: "created_at",
                ..
            } if source_value == "/repos/gt"
        ));
    }
}
