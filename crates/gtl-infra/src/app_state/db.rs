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

/// Schema v3: normalizes the `recipe_json` blob into relational tables --
/// project sources, the operation and target vocabularies, and typed columns
/// on the render log -- then rebuilds `recent_renders` with the `SQLite`
/// copy-and-swap table-rebuild procedure (create next table, copy rows, drop
/// old, rename). Pins stay inline on `recent_renders`: a pin is a per-render
/// 1:1 fact with no cross-row reuse, so a dedicated table would add a join
/// without removing any duplication.
///
/// Legacy rows are seeded through `json_extract`; rows whose JSON is malformed
/// or misshapen are dropped rather than aborting the migration, since a failed
/// migration would brick every later open of the app database.
const SCHEMA_V3: &str = "
CREATE TABLE project_sources (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  kind       TEXT NOT NULL CHECK (kind IN ('directory', 'remote')),
  value      TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT,
  UNIQUE (kind, value)
) STRICT;

CREATE TABLE render_operations (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
) STRICT;

INSERT INTO render_operations (id, name) VALUES
  (1, 'diff'),
  (2, 'merge_diff');

CREATE TABLE render_targets (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
) STRICT;

INSERT INTO render_targets (id, name) VALUES
  (1, 'unpushed'),
  (2, 'base'),
  (3, 'range'),
  (4, 'merge'),
  (5, 'last');

-- argument holds the one free parameter an operation or target carries: the
-- base rev, range expression, merge base, merge-diff base, or last-count.
CREATE TABLE recent_renders_next (
  id           INTEGER PRIMARY KEY,
  source_id    INTEGER NOT NULL REFERENCES project_sources (id),
  operation_id INTEGER NOT NULL REFERENCES render_operations (id),
  target_id    INTEGER REFERENCES render_targets (id),
  argument     TEXT,
  pinned_base  TEXT,
  pinned_head  TEXT,
  recipe_name  TEXT,
  title        TEXT NOT NULL,
  repo_name    TEXT NOT NULL,
  range_label  TEXT NOT NULL,
  rendered_at  TEXT NOT NULL,
  CHECK ((pinned_base IS NULL) = (pinned_head IS NULL)),
  -- Only the diff operation (seeded id 1) takes a target.
  CHECK ((operation_id = 1) = (target_id IS NOT NULL))
) STRICT;

INSERT INTO project_sources (kind, value, created_at, updated_at)
SELECT 'directory',
       json_extract(recipe_json, '$.source.value'),
       MIN(rendered_at),
       MAX(rendered_at)
FROM recent_renders
WHERE json_valid(recipe_json)
  AND json_extract(recipe_json, '$.source.value') IS NOT NULL
GROUP BY json_extract(recipe_json, '$.source.value');

INSERT INTO recent_renders_next
  (id, source_id, operation_id, target_id, argument,
   pinned_base, pinned_head, recipe_name,
   title, repo_name, range_label, rendered_at)
SELECT r.id,
       s.id,
       o.id,
       t.id,
       CAST(coalesce(
         json_extract(r.recipe_json, '$.op.target.rev'),
         json_extract(r.recipe_json, '$.op.target.range'),
         json_extract(r.recipe_json, '$.op.target.base'),
         json_extract(r.recipe_json, '$.op.target.count'),
         json_extract(r.recipe_json, '$.op.base')
       ) AS TEXT),
       coalesce(
         json_extract(r.recipe_json, '$.op.target.pinned.base'),
         json_extract(r.recipe_json, '$.op.pinned.base')
       ),
       coalesce(
         json_extract(r.recipe_json, '$.op.target.pinned.head'),
         json_extract(r.recipe_json, '$.op.pinned.head')
       ),
       json_extract(r.recipe_json, '$.name'),
       r.title, r.repo_name, r.range_label, r.rendered_at
FROM recent_renders r
JOIN project_sources s
  ON s.kind = 'directory'
 AND s.value = json_extract(r.recipe_json, '$.source.value')
JOIN render_operations o
  ON o.name = replace(json_extract(r.recipe_json, '$.op.op'), '-', '_')
LEFT JOIN render_targets t
  ON t.name = json_extract(r.recipe_json, '$.op.target.target')
WHERE json_valid(r.recipe_json)
  AND (o.name != 'diff' OR t.id IS NOT NULL)
  AND ((coalesce(
          json_extract(r.recipe_json, '$.op.target.pinned.base'),
          json_extract(r.recipe_json, '$.op.pinned.base')
        ) IS NULL)
       =
       (coalesce(
          json_extract(r.recipe_json, '$.op.target.pinned.head'),
          json_extract(r.recipe_json, '$.op.pinned.head')
        ) IS NULL));

DROP TABLE recent_renders;

ALTER TABLE recent_renders_next RENAME TO recent_renders;
";

/// Schema v4 defines recent-render identity and its lookup indexes.
const SCHEMA_V4: &str = "
DELETE FROM recent_renders
WHERE id NOT IN (
  SELECT MAX(id)
  FROM recent_renders
  GROUP BY source_id,
           repo_name,
           coalesce(pinned_base, X''),
           coalesce(pinned_head, X'')
);

DELETE FROM project_sources
WHERE id NOT IN (SELECT source_id FROM recent_renders);

CREATE UNIQUE INDEX recent_renders_fingerprint_idx
ON recent_renders (
  source_id,
  repo_name,
  coalesce(pinned_base, X''),
  coalesce(pinned_head, X'')
);

CREATE INDEX recent_renders_repo_name_idx
ON recent_renders (repo_name);

CREATE INDEX project_sources_value_idx
ON project_sources (value);
";

const SCHEMA_V5: &str = "
DELETE FROM recent_renders WHERE operation_id = 3;
DELETE FROM render_operations WHERE id = 3;
DELETE FROM project_sources WHERE id NOT IN (SELECT source_id FROM recent_renders);
";

const MIGRATIONS_SLICE: &[M<'_>] = &[
    M::up(SCHEMA_V1),
    M::up(SCHEMA_V2),
    M::up(SCHEMA_V3),
    M::up(SCHEMA_V4),
    M::up(SCHEMA_V5),
];
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

    // ! Fresh-file race: two server starts can both open a brand-new gtl.db
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
        match initialize_app_db(&mut conn) {
            Ok(()) => {
                last_err = None;
                break;
            }
            Err(err) => {
                last_err = Some(err);
                wait_before_init_retry(attempt);
            }
        }
    }
    if let Some(err) = last_err {
        return Err(err);
    }
    Ok(conn)
}

fn initialize_app_db(conn: &mut Connection) -> anyhow::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    MIGRATIONS.to_latest(conn)?;
    Ok(())
}

fn wait_before_init_retry(attempt: u32) {
    if attempt < INIT_RETRY_ATTEMPTS {
        std::thread::sleep(INIT_RETRY_BACKOFF * attempt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_database_open(
        root: std::path::PathBuf,
        barrier: std::sync::Arc<std::sync::Barrier>,
    ) -> std::thread::JoinHandle<anyhow::Result<()>> {
        std::thread::spawn(move || {
            barrier.wait();
            open_app_db(&root).map(|_| ())
        })
    }

    #[test]
    fn connection_pragmas_are_applied() {
        let directory = tempfile::tempdir().unwrap();
        let connection = open_app_db(directory.path()).unwrap();

        let journal_mode: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        let foreign_keys: i64 = connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();

        assert_eq!(journal_mode, "wal");
        assert_eq!(foreign_keys, 1);
    }

    #[test]
    fn corrupt_database_open_returns_an_error() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("gtl.db"), "not a database").unwrap();

        let result = open_app_db(directory.path());

        assert!(result.is_err());
    }

    #[test]
    fn migration_v2_drops_settings_and_preserves_runtime_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gtl.db");
        let mut connection = Connection::open(&path).unwrap();
        Migrations::from_slice(&[M::up(SCHEMA_V1)])
            .to_latest(&mut connection)
            .unwrap();
        connection
            .execute(
                "INSERT INTO settings (key, value) VALUES ('theme', 'light')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO live_views (source_kind, source_value, display_name, created_at) VALUES ('local', '/repo', 'Repo', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO recent_renders (recipe_json, title, repo_name, kind, range_label, rendered_at) VALUES ('{\"source\":{\"kind\":\"local_repo\",\"value\":\"/repo\"},\"op\":{\"op\":\"diff\",\"target\":{\"target\":\"unpushed\"}}}', 'Render', 'repo', 'diff', 'main..HEAD', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        drop(connection);

        let connection = open_app_db(directory.path()).unwrap();
        let user_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let settings_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'settings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let live_view_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM live_views", [], |row| row.get(0))
            .unwrap();
        let recent_render_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM recent_renders", [], |row| row.get(0))
            .unwrap();

        assert_eq!(user_version, 5);
        assert_eq!(settings_table_count, 0);
        assert_eq!(live_view_count, 1);
        assert_eq!(recent_render_count, 1);
    }

    /// Applies v1+v2, inserts legacy `recipe_json` rows (one per interesting
    /// shape, plus one malformed), and returns a v3-migrated connection.
    fn migrated_from_legacy_rows(directory: &Path) -> Connection {
        let path = directory.join("gtl.db");
        let mut connection = Connection::open(&path).unwrap();
        Migrations::from_slice(&[M::up(SCHEMA_V1), M::up(SCHEMA_V2)])
            .to_latest(&mut connection)
            .unwrap();
        let legacy_rows = [
            (
                r#"{"source":{"kind":"local_repo","value":"/repos/gt"},"op":{"op":"diff","target":{"target":"unpushed","pinned":{"base":"aaa","head":"bbb"}}}}"#,
                "2026-01-01T00:00:00Z",
            ),
            (
                r#"{"source":{"kind":"LocalRepo","value":"/repos/gt"},"op":{"op":"merge-diff","base":"main"}}"#,
                "2026-01-03T00:00:00Z",
            ),
            (
                r#"{"source":{"kind":"local_repo","value":"/repos/other"},"op":{"op":"diff","target":{"target":"last","count":3}},"name":"named"}"#,
                "2026-01-02T00:00:00Z",
            ),
            ("not json", "2026-01-04T00:00:00Z"),
        ];
        for (recipe_json, rendered_at) in legacy_rows {
            connection
                .execute(
                    "INSERT INTO recent_renders (recipe_json, title, repo_name, kind, range_label, rendered_at) \
                     VALUES (?1, 't', 'r', 'k', 'l', ?2)",
                    rusqlite::params![recipe_json, rendered_at],
                )
                .unwrap();
        }
        drop(connection);
        open_app_db(directory).unwrap()
    }

    /// Pins the v3 source seeding: sources dedupe by (kind, value) with
    /// first/last-seen timestamps, and malformed rows contribute nothing.
    #[test]
    fn migration_v3_seeds_deduped_project_sources_from_legacy_recipe_json() {
        let directory = tempfile::tempdir().unwrap();
        let connection = migrated_from_legacy_rows(directory.path());
        let sources: Vec<(String, String, String, Option<String>)> = connection
            .prepare(
                "SELECT kind, value, created_at, updated_at FROM project_sources ORDER BY value",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            sources,
            vec![
                (
                    "directory".into(),
                    "/repos/gt".into(),
                    "2026-01-01T00:00:00Z".into(),
                    Some("2026-01-03T00:00:00Z".into()),
                ),
                (
                    "directory".into(),
                    "/repos/other".into(),
                    "2026-01-02T00:00:00Z".into(),
                    Some("2026-01-02T00:00:00Z".into()),
                ),
            ]
        );
    }

    type SeededRender = (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );

    /// Pins the v3 render seeding: rows land on the relational columns with
    /// legacy serde tags normalized, and undecodable rows drop instead of
    /// aborting the migration.
    #[test]
    fn migration_v3_seeds_relational_renders_from_legacy_recipe_json() {
        let directory = tempfile::tempdir().unwrap();
        let connection = migrated_from_legacy_rows(directory.path());
        let renders: Vec<SeededRender> = connection
            .prepare(
                "SELECT s.value, o.name, t.name, r.argument, r.pinned_base, r.pinned_head, r.recipe_name
                 FROM recent_renders r
                 JOIN project_sources s ON s.id = r.source_id
                 JOIN render_operations o ON o.id = r.operation_id
                 LEFT JOIN render_targets t ON t.id = r.target_id
                 ORDER BY r.id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            renders,
            vec![
                (
                    "/repos/gt".into(),
                    "diff".into(),
                    Some("unpushed".into()),
                    None,
                    Some("aaa".into()),
                    Some("bbb".into()),
                    None,
                ),
                (
                    "/repos/gt".into(),
                    "merge_diff".into(),
                    None,
                    Some("main".into()),
                    None,
                    None,
                    None,
                ),
                (
                    "/repos/other".into(),
                    "diff".into(),
                    Some("last".into()),
                    Some("3".into()),
                    None,
                    None,
                    Some("named".into()),
                ),
            ]
        );
    }

    #[test]
    fn migration_v4_deduplicates_fingerprints_and_adds_indexes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gtl.db");
        let mut connection = Connection::open(path).unwrap();
        Migrations::from_slice(&MIGRATIONS_SLICE[..3])
            .to_latest(&mut connection)
            .unwrap();
        connection
            .execute_batch(
                "INSERT INTO project_sources (id, kind, value, created_at) VALUES
                   (1, 'directory', '/repos/gt', '2026-07-01T00:00:00Z'),
                   (2, 'remote', '/repos/gt', '2026-07-01T00:00:00Z'),
                   (3, 'directory', '/repos/other', '2026-07-01T00:00:00Z'),
                   (4, 'directory', '/repos/orphan', '2026-07-01T00:00:00Z');
                 INSERT INTO recent_renders
                   (id, source_id, operation_id, target_id, pinned_base, pinned_head,
                    title, repo_name, range_label, rendered_at)
                 VALUES
                   (1, 1, 1, 1, 'base', 'head', 'older duplicate', 'gt', 'base..head',
                    '2026-07-01T00:00:00Z'),
                   (2, 1, 1, 1, 'base', 'head', 'newer duplicate', 'gt', 'base..head',
                    '2026-07-02T00:00:00Z'),
                   (3, 2, 1, 1, 'base', 'head', 'source kind', 'gt', 'base..head',
                    '2026-07-02T00:00:00Z'),
                   (4, 3, 1, 1, 'base', 'head', 'source value', 'gt', 'base..head',
                    '2026-07-02T00:00:00Z'),
                   (5, 1, 1, 1, 'base', 'head', 'repository', 'other', 'base..head',
                    '2026-07-02T00:00:00Z'),
                   (6, 1, 1, 1, 'other-base', 'head', 'base', 'gt', 'base..head',
                    '2026-07-02T00:00:00Z'),
                   (7, 1, 1, 1, 'base', 'other-head', 'head', 'gt', 'base..head',
                    '2026-07-02T00:00:00Z');",
            )
            .unwrap();
        drop(connection);

        let connection = open_app_db(directory.path()).unwrap();
        let render_ids = connection
            .prepare("SELECT id FROM recent_renders ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let source_ids = connection
            .prepare("SELECT id FROM project_sources ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let index_names = connection
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'index'
                   AND name IN (
                     'project_sources_value_idx',
                     'recent_renders_fingerprint_idx',
                     'recent_renders_repo_name_idx'
                   )
                 ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let user_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();

        assert_eq!(render_ids, vec![2, 3, 4, 5, 6, 7]);
        assert_eq!(source_ids, vec![1, 2, 3]);
        assert_eq!(
            index_names,
            vec![
                "project_sources_value_idx".to_owned(),
                "recent_renders_fingerprint_idx".to_owned(),
                "recent_renders_repo_name_idx".to_owned(),
            ]
        );
        assert_eq!(user_version, 5);
    }

    /// Two processes can open a fresh database concurrently; both
    /// must succeed. Deterministically hitting the migration race is not
    /// possible from a test, but this pins the contract the retry guard serves.
    #[test]
    fn concurrent_first_open_both_succeed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| spawn_database_open(root.clone(), std::sync::Arc::clone(&barrier)))
            .collect();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
    }
}
