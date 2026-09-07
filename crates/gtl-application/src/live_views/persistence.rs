use gtl_models::{live_views::LiveSource, paths::ProjectName, timestamps::MachineTimestamp};
use rusqlite::{Connection, params};

/// One saved live view, keyed by its source identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveViewRecord {
    pub source: LiveSource,
    pub comparison: gtl_models::live_views::LiveComparison,
    pub display_name: ProjectName,
    pub created_at: MachineTimestamp,
    pub last_opened_at: Option<MachineTimestamp>,
}

pub(super) fn delete_live_view(
    connection: &Connection,
    source: &LiveSource,
    comparison: gtl_models::live_views::LiveComparison,
) -> anyhow::Result<()> {
    let mut statement = connection.prepare_cached(
        "DELETE FROM live_views WHERE source_kind = ?1 AND source_value = ?2 AND comparison = ?3",
    )?;
    let source_value = source.value();
    statement.execute(params![source.kind(), source_value, comparison.as_str()])?;
    Ok(())
}

#[cfg(test)]
pub(super) fn store_test() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE live_views (
          id             INTEGER PRIMARY KEY,
          source_kind    TEXT NOT NULL,
          source_value   TEXT NOT NULL,
          display_name   TEXT NOT NULL,
          created_at     TEXT NOT NULL,
          last_opened_at TEXT,
          comparison TEXT NOT NULL DEFAULT 'unpushed_commits' CHECK (comparison IN ('local_changes', 'unpushed_commits')),
          UNIQUE (source_kind, source_value, comparison)
        ) STRICT;",
        )
        .unwrap();
    connection
}
