use gtl_models::{live_views::LiveSource, paths::ProjectName, timestamps::MachineTimestamp};
#[cfg(test)]
use rusqlite::Connection;

/// One saved live view, keyed by its source identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveViewRecord {
    pub source: LiveSource,
    pub comparison: gtl_models::live_views::LiveComparison,
    pub display_name: ProjectName,
    pub created_at: MachineTimestamp,
    pub last_opened_at: Option<MachineTimestamp>,
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
