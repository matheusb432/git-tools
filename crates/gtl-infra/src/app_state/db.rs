//! Initializes one process-owned app-state connection with its pragmas and migrations.

use std::{path::Path, sync::LazyLock, time::Duration};

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

const SCHEMA_V6: &str = "
CREATE TABLE live_views_next (
  id INTEGER PRIMARY KEY,
  source_kind TEXT NOT NULL,
  source_value TEXT NOT NULL,
  display_name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  last_opened_at TEXT,
  comparison TEXT NOT NULL DEFAULT 'unpushed_commits' CHECK (comparison IN ('local_changes', 'unpushed_commits')),
  UNIQUE (source_kind, source_value, comparison)
) STRICT;
INSERT INTO live_views_next (id, source_kind, source_value, display_name, created_at, last_opened_at)
SELECT id, source_kind, source_value, display_name, created_at, last_opened_at FROM live_views;
DROP TABLE live_views;
ALTER TABLE live_views_next RENAME TO live_views;
CREATE TABLE project_render_recency (
  source_value TEXT PRIMARY KEY,
  rendered_at TEXT NOT NULL
) STRICT;
INSERT INTO project_render_recency (source_value, rendered_at)
SELECT value, coalesce(updated_at, created_at) FROM project_sources WHERE kind = 'directory';
";

static MIGRATIONS_SLICE: LazyLock<[M<'static>; 14]> = LazyLock::new(|| {
    [
        M::up(SCHEMA_V1),
        M::up(SCHEMA_V2),
        M::up(SCHEMA_V3),
        M::up(SCHEMA_V4),
        M::up(SCHEMA_V5),
        M::up(SCHEMA_V6),
        M::up(include_str!("../../db/migrations/0007_own_projects.sql")),
        M::up(include_str!(
            "../../db/migrations/0008_project_comparison_branch.sql"
        )),
        M::up(include_str!(
            "../../db/migrations/0009_snapshot_projects.sql"
        )),
        M::up(include_str!(
            "../../db/migrations/0010_pinned_viewer_tabs.sql"
        )),
        M::up(include_str!(
            "../../db/migrations/0011_project_status_index.sql"
        )),
        M::up(include_str!(
            "../../db/migrations/0012_render_attempt_status.sql"
        )),
        M::up(include_str!(
            "../../db/migrations/0013_remove_legacy_project_fields.sql"
        )),
        M::up_with_hook(
            include_str!("../../db/migrations/0014_absolute_project_sources.sql"),
            migrate_absolute_project_sources,
        ),
    ]
});
static MIGRATIONS: LazyLock<Migrations<'static>> =
    LazyLock::new(|| Migrations::from_slice(&MIGRATIONS_SLICE[..]));

fn migrate_absolute_project_sources(
    tx: &rusqlite::Transaction<'_>,
) -> rusqlite_migration::HookResult {
    use rusqlite::params;
    use rusqlite_migration::HookError;
    let mut statement = tx.prepare(
        "SELECT source_id, source_value FROM project_sources WHERE source_kind = 'directory'",
    )?;
    let sources = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let home = if sources.iter().any(|(_, source)| source.starts_with("~/")) {
        Some(
            directories::BaseDirs::new()
                .ok_or_else(|| HookError::Hook("home directory unavailable".into()))?
                .home_dir()
                .to_path_buf(),
        )
    } else {
        None
    };
    for (source_id, source_value) in sources {
        let Some(relative) = source_value.strip_prefix("~/") else {
            gtl_models::projects::catalogue::ProjectDirectorySource::try_new(source_value)
                .map_err(|error| HookError::Hook(error.to_string()))?;
            continue;
        };
        if relative.is_empty()
            || relative.contains(['~', '\\', ':', '\0'])
            || relative
                .split('/')
                .any(|part| matches!(part, "" | "." | ".."))
        {
            return Err(HookError::Hook(format!(
                "invalid legacy project source: {source_value}"
            )));
        }
        let absolute = home
            .as_ref()
            .ok_or_else(|| HookError::Hook("home directory unavailable".into()))?
            .join(relative);
        let absolute = absolute
            .to_str()
            .ok_or_else(|| HookError::Hook("home path is not UTF-8".into()))?;
        gtl_models::projects::catalogue::ProjectDirectorySource::try_new(absolute.to_owned())
            .map_err(|error| HookError::Hook(error.to_string()))?;
        tx.execute(
            "UPDATE project_sources SET source_value = ?1 WHERE source_id = ?2",
            params![absolute, source_id],
        )?;
    }
    Ok(())
}

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
    fn migration_retires_saved_standalone_working_tree_views() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        Migrations::from_slice(&MIGRATIONS_SLICE[..8])
            .to_latest(&mut connection)
            .unwrap();
        connection.execute_batch("INSERT INTO live_views (source_kind, source_value, display_name, created_at, comparison) VALUES
            ('LocalRepo', '/repos/one', 'one', '2026-09-10T00:00:00Z', 'local_changes'),
            ('LocalRepo', '/repos/one', 'one', '2026-09-10T00:00:00Z', 'unpushed_commits'),
            ('LocalRepo', '/repos/two', 'two', '2026-09-10T00:00:00Z', 'local_changes');").unwrap();
        MIGRATIONS.to_latest(&mut connection).unwrap();
        let comparisons = connection
            .prepare("SELECT comparison FROM live_views ORDER BY source_value")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(comparisons, ["unpushed_commits", "unpushed_commits"]);
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
    fn migration_v8_defaults_existing_and_new_projects_without_losing_metadata() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        Migrations::from_slice(&MIGRATIONS_SLICE[..7])
            .to_latest(&mut connection)
            .unwrap();
        connection.execute_batch("INSERT INTO project_sources (source_id, source_kind, source_value) VALUES (1, 'directory', '~/tools/example');
            INSERT INTO projects (id, source_id, title, mux_session_name, affiliation)
            VALUES ('PRJ', 1, 'Example', 'example', 'personal');
            INSERT INTO project_groups (project_id, group_name) VALUES ('PRJ', 'tools');").unwrap();
        MIGRATIONS.to_latest(&mut connection).unwrap();
        let row: (String, String) = connection
            .query_row(
                "SELECT title, comparison_branch FROM projects WHERE id = 'PRJ'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("Example".to_owned(), "main".to_owned()));
        assert_eq!(
            connection
                .query_row(
                    "SELECT group_name FROM project_groups WHERE project_id = 'PRJ'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "tools"
        );
        connection.execute_batch("INSERT INTO project_sources (source_id, source_kind, source_value) VALUES (2, 'directory', '/tools/new');
            INSERT INTO projects (id, source_id, title) VALUES ('NEW', 2, 'New');").unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT comparison_branch FROM projects WHERE id = 'NEW'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "main"
        );
    }

    fn seed_v12_project_catalogue(connection: &Connection) {
        for index in 0_u8..35 {
            let id = format!(
                "P{}{}",
                char::from(b'A' + index / 26),
                char::from(b'A' + index % 26)
            );
            let source_id = i64::from(index) + 1;
            let source = format!("~/projects/project-{index}");
            let title = format!("Project {index}");
            connection
                .execute(
                    "INSERT INTO project_sources (source_id, source_kind, source_value) VALUES (?1, 'directory', ?2)",
                    rusqlite::params![source_id, source],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO projects (id, source_id, title, mux_session_name, affiliation, git_remote, color, export_include_in_all, created_at, paused_at, unmanaged_at, comparison_branch) VALUES (?1, ?2, ?3, ?4, 'personal', 'git@example.test:project.git', '#123abc', 1, '2026-01-01T00:00:00.000Z', NULL, NULL, 'main')",
                    rusqlite::params![id, source_id, title, format!("project-{index}")],
                )
                .unwrap();
        }
        connection
            .execute(
                "INSERT INTO project_groups (project_id, group_name) VALUES ('PAA', 'tools')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO project_status_index (project_id, source_id, comparison_branch, commits_ahead, tracked_changes, untracked_changes, checked_at) VALUES ('PAA', 1, 'main', 2, 1, 0, 1780000000)",
                [],
            )
            .unwrap();
        connection
            .execute_batch(
                "INSERT INTO render_sources (id, kind, value, created_at)
             VALUES (1, 'directory', '/repos/project-0', '2026-01-01T00:00:00Z');
             INSERT INTO recent_renders
               (id, source_id, operation_id, target_id, title, repo_name,
                range_label, rendered_at, project_id)
             VALUES (1, 1, 1, 1, 'Snapshot', 'project-0', 'main..HEAD',
                     '2026-01-01T00:00:00Z', 'PAA');",
            )
            .unwrap();
    }

    #[test]
    fn migration_v13_removes_legacy_project_fields_and_preserves_v12_projects() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        Migrations::from_slice(&MIGRATIONS_SLICE[..12])
            .to_latest(&mut connection)
            .unwrap();

        seed_v12_project_catalogue(&connection);

        Migrations::from_slice(&MIGRATIONS_SLICE[..13])
            .to_latest(&mut connection)
            .unwrap();

        let project_count: i64 = connection
            .query_row("SELECT count(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(project_count, 35);
        let first_source: (i64, String) = connection
            .query_row(
                "SELECT source_id, source_value FROM project_sources WHERE source_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(first_source, (1, "~/projects/project-0".into()));
        assert_eq!(
            connection
                .query_row(
                    "SELECT title, git_remote, color, export_include_in_all, created_at, comparison_branch FROM projects WHERE id = 'PAA'",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<i64>>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    }
                )
                .unwrap(),
            (
                "Project 0".to_owned(),
                Some("git@example.test:project.git".to_owned()),
                Some("#123abc".to_owned()),
                Some(1),
                "2026-01-01T00:00:00.000Z".to_owned(),
                "main".to_owned()
            )
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT group_name FROM project_groups WHERE project_id = 'PAA'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "tools"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_id, commits_ahead FROM project_status_index WHERE project_id = 'PAA'",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                )
                .unwrap(),
            (1, 2)
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT project_id FROM recent_renders WHERE id = 1",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "PAA"
        );
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        let project_columns = connection
            .prepare("PRAGMA table_info(projects)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            !project_columns
                .iter()
                .any(|column| { matches!(column.as_str(), "mux_session_name" | "affiliation") })
        );
    }

    #[test]
    fn migration_v14_expands_sources_in_place_and_preserves_project_data() {
        let mut connection = Connection::open_in_memory().unwrap();
        let other_directory = tempfile::tempdir().unwrap();
        let other_path = other_directory.path().join("other");
        let other_source = other_path.to_str().unwrap().to_owned();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        Migrations::from_slice(&MIGRATIONS_SLICE[..13])
            .to_latest(&mut connection)
            .unwrap();
        connection
            .execute_batch(
                "INSERT INTO project_sources (source_id, source_kind, source_value) VALUES
               (7, 'directory', '~/tools/git-tools'),
               (8, 'directory', '/placeholder');
             INSERT INTO projects (id, source_id, title, paused_at, comparison_branch)
               VALUES ('GTL', 7, 'Git Tools', '2026-01-01T00:00:00.000Z', 'develop'),
                      ('OTH', 8, 'Other', NULL, 'main');
             INSERT INTO project_groups (project_id, group_name) VALUES ('GTL', 'tools');
             INSERT INTO project_status_index
               (project_id, source_id, comparison_branch, commits_ahead, checked_at)
               VALUES ('GTL', 7, 'develop', 3, 1780000000);
             INSERT INTO render_sources (id, kind, value, created_at)
               VALUES (1, 'directory', '/repos/gt', '2026-01-01T00:00:00Z');
             INSERT INTO recent_renders
               (id, source_id, operation_id, target_id, title, repo_name,
                range_label, rendered_at, project_id)
               VALUES (1, 1, 1, 1, 'Snapshot', 'gt', 'main..HEAD',
                       '2026-01-01T00:00:00Z', 'GTL');",
            )
            .unwrap();
        connection
            .execute(
                "UPDATE project_sources SET source_value = ?1 WHERE source_id = 8",
                [&other_source],
            )
            .unwrap();

        MIGRATIONS.to_latest(&mut connection).unwrap();
        let home = directories::BaseDirs::new().unwrap();
        let expected = home
            .home_dir()
            .join("tools/git-tools")
            .to_str()
            .unwrap()
            .to_owned();
        let sources = connection
            .prepare("SELECT source_id, source_value FROM project_sources ORDER BY source_id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(sources, vec![(7, expected), (8, other_source)]);
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_id, paused_at, comparison_branch FROM projects WHERE id = 'GTL'",
                    [],
                    |row| Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?
                    ))
                )
                .unwrap(),
            (7, "2026-01-01T00:00:00.000Z".into(), "develop".into())
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT project_id FROM recent_renders WHERE id = 1",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "GTL"
        );
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
    }

    #[test]
    fn fresh_schema_v14_accepts_absolute_sources_outside_home() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("new").to_str().unwrap().to_owned();
        let connection = open_app_db(directory.path()).unwrap();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 14);
        connection
            .execute(
                "INSERT INTO project_sources (source_kind, source_value) VALUES ('directory', ?1)",
                [&source],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO projects (id, source_id, title) VALUES ('NEW', last_insert_rowid(), 'New')",
                [],
            )
            .unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_value FROM active_projects WHERE id = 'NEW'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            source
        );
    }

    #[test]
    fn migration_v14_rejects_relative_sources_without_partial_updates() {
        let mut connection = Connection::open_in_memory().unwrap();
        Migrations::from_slice(&MIGRATIONS_SLICE[..13])
            .to_latest(&mut connection)
            .unwrap();
        connection
            .execute_batch(
                "INSERT INTO project_sources (source_id, source_kind, source_value) VALUES
               (1, 'directory', '~/tools/git-tools'),
               (2, 'directory', 'relative/repository');",
            )
            .unwrap();
        assert!(MIGRATIONS.to_latest(&mut connection).is_err());
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 13);
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_value FROM project_sources WHERE source_id = 1",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "~/tools/git-tools"
        );
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
                "INSERT INTO settings (key, value) VALUES ('theme', 'glacier')",
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

        assert_eq!(user_version, 14);
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
                "SELECT kind, value, created_at, updated_at FROM render_sources ORDER BY value",
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
                 JOIN render_sources s ON s.id = r.source_id
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
            .prepare("SELECT id FROM render_sources ORDER BY id")
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
                     'render_sources_value_idx',
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
                "recent_renders_fingerprint_idx".to_owned(),
                "recent_renders_repo_name_idx".to_owned(),
                "render_sources_value_idx".to_owned(),
            ]
        );
        assert_eq!(user_version, 14);
    }

    #[test]
    fn migration_v12_defaults_existing_renders_and_allows_concurrent_attempts() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        Migrations::from_slice(&MIGRATIONS_SLICE[..11])
            .to_latest(&mut connection)
            .unwrap();
        connection
            .execute_batch(
                "INSERT INTO render_sources (id, kind, value, created_at)
                 VALUES (1, 'directory', '/repos/gt', '2026-09-19T00:00:00Z');
                 INSERT INTO recent_renders
                   (id, source_id, operation_id, target_id, pinned_base, pinned_head,
                    title, repo_name, range_label, rendered_at)
                 VALUES
                   (1, 1, 1, 1, 'base', 'head', 'existing', 'gt', 'base..head',
                    '2026-09-19T00:00:00Z');",
            )
            .unwrap();

        MIGRATIONS.to_latest(&mut connection).unwrap();

        let status: String = connection
            .query_row(
                "SELECT render_status FROM recent_renders WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "success");
        connection
            .execute_batch(
                "INSERT INTO recent_renders
                   (id, source_id, operation_id, target_id, pinned_base, pinned_head,
                    title, repo_name, range_label, rendered_at, render_status)
                 VALUES
                   (2, 1, 1, 1, 'base', 'head', 'pending one', 'gt', 'base..head',
                    '2026-09-19T00:00:01Z', 'pending'),
                   (3, 1, 1, 1, 'base', 'head', 'pending two', 'gt', 'base..head',
                    '2026-09-19T00:00:02Z', 'pending');
                 UPDATE recent_renders SET render_status = 'error' WHERE id = 2;
                 INSERT INTO render_errors (recent_render_id, error_code, error_detail)
                 VALUES
                   (2, 'render_failed', 'top-level failure'),
                   (2, 'render_failed', 'source detail');",
            )
            .unwrap();
        assert!(
            connection
                .execute(
                    "UPDATE recent_renders SET render_status = 'success' WHERE id = 3",
                    [],
                )
                .is_err()
        );
        let violations = connection
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query_map([], |_| Ok(()))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(violations.is_empty());
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
