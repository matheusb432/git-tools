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
    // ! Fresh-file race: daemon and viewer can both open a brand-new gtl.db and
    // ! both attempt migration v1; the loser errors ("table already exists" /
    // ! SQLITE_BUSY). Its retry re-reads user_version, sees the winner's bump,
    // ! and no-ops. A genuine migration failure fails both attempts and surfaces.
    if MIGRATIONS.to_latest(&mut conn).is_err() {
        MIGRATIONS.to_latest(&mut conn)?;
    }
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two processes (daemon + viewer) can open a fresh db concurrently; both
    /// must succeed. Deterministically hitting the migration race is not
    /// possible from a test, but this pins the contract the retry guard serves.
    #[test]
    fn concurrent_first_open_both_succeed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let root = root.clone();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    open_app_db(&root).map(|_| ())
                })
            })
            .collect();
        for handle in handles {
            handle
                .join()
                .expect("no panic")
                .expect("open must survive the fresh-file race");
        }
    }
}
