#[cfg(test)]
use crate::testing::AppStateStoreTest;

/// One saved live view, keyed by its source identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveViewRecord {
    pub source_kind: String,
    pub source_value: String,
    pub display_name: String,
    pub created_at: String,
    pub last_opened_at: Option<String>,
}

#[cfg(test)]
pub(crate) fn store_test() -> AppStateStoreTest {
    AppStateStoreTest::new(
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
    .expect("live-view test store")
}
