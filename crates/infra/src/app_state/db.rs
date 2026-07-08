//! The app-state database: one `SQLite` file under the data root, opened
//! per call through [`open_app_db`] — the single place the connection
//! pragmas (WAL, busy timeout, foreign keys) and migrations are applied.

use std::{path::Path, time::Duration};

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

/// Schema v1: live views (identity-deduped), settings, and the recent-render
/// log. STRICT so declared types are enforced, not affinity hints.
const SCHEMA_V1: &str = "
CREATE TABLE live_views (
  id             INTEGER PRIMARY KEY,
  source_kind    TEXT NOT NULL,
  source_value   TEXT NOT NULL,
  display_name   TEXT NOT NULL,
  created_at     TEXT NOT NULL,
  last_opened_at TEXT,
  UNIQUE (source_kind, source_value)
) STRICT;

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;

CREATE TABLE recent_renders (
  id          INTEGER PRIMARY KEY,
  recipe_json TEXT NOT NULL,
  title       TEXT NOT NULL,
  repo_name   TEXT NOT NULL,
  kind        TEXT NOT NULL,
  range_label TEXT NOT NULL,
  rendered_at TEXT NOT NULL
) STRICT;
";

const MIGRATIONS_SLICE: &[M<'_>] = &[M::up(SCHEMA_V1)];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATIONS_SLICE);

/// Open (creating if needed) the app-state db under `data_root`, apply the
/// connection pragmas, and migrate to the latest schema version.
pub(crate) fn open_app_db(data_root: &Path) -> anyhow::Result<Connection> {
    std::fs::create_dir_all(data_root)?;
    let mut conn = Connection::open(data_root.join("gtl.db"))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    MIGRATIONS.to_latest(&mut conn)?;
    Ok(conn)
}
