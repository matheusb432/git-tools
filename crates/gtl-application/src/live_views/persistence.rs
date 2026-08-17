use gtl_models::{live_views::LiveSource, paths::ProjectName, timestamps::MachineTimestamp};
#[cfg(test)]
use rusqlite::Connection;

/// One saved live view, keyed by its source identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveViewRecord {
    pub source: LiveSource,
    pub display_name: ProjectName,
    pub created_at: MachineTimestamp,
    pub last_opened_at: Option<MachineTimestamp>,
}

#[cfg(test)]
pub(super) fn store_test() -> Connection {
    let connection = Connection::open_in_memory().expect("live-view test connection");
    connection
        .execute_batch(
            "CREATE TABLE live_views (
          id             INTEGER PRIMARY KEY,
          source_kind    TEXT NOT NULL,
          source_value   TEXT NOT NULL,
          display_name   TEXT NOT NULL,
          created_at     TEXT NOT NULL,
          last_opened_at TEXT,
          UNIQUE (source_kind, source_value)
        ) STRICT;",
        )
        .expect("live-view test schema");
    connection
}
