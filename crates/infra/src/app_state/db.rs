//! Initializes one process-owned app-state connection with its pragmas and migrations.

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

const SCHEMA_V2: &str = "DROP TABLE settings;";

const MIGRATIONS_SLICE: &[M<'_>] = &[M::up(SCHEMA_V1), M::up(SCHEMA_V2)];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATIONS_SLICE);

/// Init-sequence retry ceiling: bounded well under the 5s `busy_timeout` so a
/// genuinely failing pragma/migration still surfaces promptly.
const INIT_RETRY_ATTEMPTS: u32 = 8;
/// Backoff between init retries, linear in the attempt number (50ms, 100ms,
/// ..., 350ms), for a worst-case total wait of ~1.4s.
const INIT_RETRY_BACKOFF: Duration = Duration::from_millis(50);

/// Opens one app-state connection under `data_root` and initializes its schema policy.
pub(crate) fn open_app_db(data_root: &Path) -> anyhow::Result<Connection> {
    std::fs::create_dir_all(data_root)?;
    let mut conn = Connection::open(data_root.join("gtl.db"))?;
    // ! busy_timeout first: every subsequent locking step (the WAL switch,
    // ! the migration) must respect it from the start.
    conn.busy_timeout(Duration::from_secs(5))?;

    // ! Fresh-file race: daemon and viewer can both open a brand-new gtl.db
    // ! and both attempt the WAL switch and migrations concurrently. SQLite
    // ! does not run the busy_timeout retry loop for the journal_mode=WAL
    // ! transition on a fresh file, so the loser can get an immediate
    // ! SQLITE_BUSY ("database is locked") right there, and even past that
    // ! point the migration can see "table already exists" if the winner
    // ! hasn't committed yet. Retry the whole init sequence with backoff: the
    // ! pragmas are idempotent and to_latest no-ops once the loser re-reads
    // ! the winner's committed user_version. Only the final attempt's error
    // ! propagates; a genuine failure keeps failing every attempt.
    let mut last_err = None;
    for attempt in 1..=INIT_RETRY_ATTEMPTS {
        let outcome: anyhow::Result<()> = (|| {
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "foreign_keys", true)?;
            MIGRATIONS.to_latest(&mut conn)?;
            Ok(())
        })();
        match outcome {
            Ok(()) => {
                last_err = None;
                break;
            }
            Err(err) => {
                last_err = Some(err);
                if attempt < INIT_RETRY_ATTEMPTS {
                    std::thread::sleep(INIT_RETRY_BACKOFF * attempt);
                }
            }
        }
    }
    if let Some(err) = last_err {
        return Err(err);
    }
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_from_empty_and_land_on_v2() {
        let directory = tempfile::tempdir().expect("temporary data root");
        let connection = open_app_db(directory.path()).expect("open app database");

        let user_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("user version");
        assert_eq!(user_version, 2);

        for (table, expected_columns) in [
            (
                "live_views",
                vec![
                    "id",
                    "source_kind",
                    "source_value",
                    "display_name",
                    "created_at",
                    "last_opened_at",
                ],
            ),
            (
                "recent_renders",
                vec![
                    "id",
                    "recipe_json",
                    "title",
                    "repo_name",
                    "kind",
                    "range_label",
                    "rendered_at",
                ],
            ),
        ] {
            let mut statement = connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .expect("prepare table info");
            let columns: Vec<String> = statement
                .query_map([], |row| row.get::<_, String>(1))
                .expect("query table info")
                .collect::<Result<_, _>>()
                .expect("decode columns");
            assert_eq!(columns, expected_columns, "columns for {table}");
        }
    }

    #[test]
    fn strict_tables_reject_lossy_types() {
        let directory = tempfile::tempdir().expect("temporary data root");
        let connection = open_app_db(directory.path()).expect("open app database");

        let error = connection
            .execute(
                "INSERT INTO live_views (source_kind, source_value, display_name, created_at)
                 VALUES ('local', '/repo', x'00', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect_err("STRICT table rejects a BLOB in a TEXT column");
        assert!(
            matches!(
                error,
                rusqlite::Error::SqliteFailure(inner, _)
                    if inner.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_DATATYPE
            ),
            "expected SQLITE_CONSTRAINT_DATATYPE, got: {error}"
        );
    }

    #[test]
    fn connection_pragmas_are_applied() {
        let directory = tempfile::tempdir().expect("temporary data root");
        let connection = open_app_db(directory.path()).expect("open app database");

        let journal_mode: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .expect("journal mode");
        let foreign_keys: i64 = connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .expect("foreign keys");

        assert_eq!(journal_mode, "wal");
        assert_eq!(foreign_keys, 1);
    }

    #[test]
    fn corrupt_database_open_returns_an_error() {
        let directory = tempfile::tempdir().expect("temporary data root");
        std::fs::write(directory.path().join("gtl.db"), "not a database")
            .expect("write corrupt database");

        let result = open_app_db(directory.path());

        assert!(result.is_err());
    }

    #[test]
    fn migration_v2_drops_settings_and_preserves_runtime_state() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("gtl.db");
        let mut connection = Connection::open(&path).expect("open v1 database");
        Migrations::from_slice(&[M::up(SCHEMA_V1)])
            .to_latest(&mut connection)
            .expect("apply v1");
        connection
            .execute(
                "INSERT INTO settings (key, value) VALUES ('theme', 'light')",
                [],
            )
            .expect("seed settings");
        connection
            .execute(
                "INSERT INTO live_views (source_kind, source_value, display_name, created_at) VALUES ('local', '/repo', 'Repo', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("seed live view");
        connection
            .execute(
                "INSERT INTO recent_renders (recipe_json, title, repo_name, kind, range_label, rendered_at) VALUES ('{}', 'Render', 'repo', 'diff', 'main..HEAD', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("seed render");
        drop(connection);

        let connection = open_app_db(directory.path()).expect("migrate database");
        let user_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("user version");
        let settings_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'settings'",
                [],
                |row| row.get(0),
            )
            .expect("settings table count");
        let live_view_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM live_views", [], |row| row.get(0))
            .expect("live view count");
        let recent_render_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM recent_renders", [], |row| row.get(0))
            .expect("recent render count");

        assert_eq!(user_version, 2);
        assert_eq!(settings_table_count, 0);
        assert_eq!(live_view_count, 1);
        assert_eq!(recent_render_count, 1);
    }

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
