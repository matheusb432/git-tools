//! `SqliteAppState`: the [`AppStateStore`] adapter over the app-state db.
//! Stateless (opens per call via `db::open_app_db`) so callers stay hermetic
//! and the daemon/viewer share the file safely under WAL.

use std::path::Path;

use application::{
    ports::{
        AppStateError, AppStateStore, LiveViewRecord, NewRecentRenderRecord, RECENT_RENDERS_CAP,
        RecentRenderRecord,
    },
    viewer::RenderHistoryId,
};
use rusqlite::{OptionalExtension, params};

use super::db::open_app_db;

/// SQLite-backed [`AppStateStore`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SqliteAppState;

struct RawRecentRenderRecord {
    id: i64,
    recipe_json: String,
    title: String,
    repo_name: String,
    kind: String,
    range_label: String,
    rendered_at: String,
}

impl RawRecentRenderRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            recipe_json: row.get(1)?,
            title: row.get(2)?,
            repo_name: row.get(3)?,
            kind: row.get(4)?,
            range_label: row.get(5)?,
            rendered_at: row.get(6)?,
        })
    }

    fn try_into_record(self) -> Result<RecentRenderRecord, AppStateError> {
        let id = RenderHistoryId::try_new(self.id)
            .map_err(|_| AppStateError::InvalidRecentRenderId { id: self.id })?;
        Ok(RecentRenderRecord {
            id,
            recipe_json: self.recipe_json,
            title: self.title,
            repo_name: self.repo_name,
            kind: self.kind,
            range_label: self.range_label,
            rendered_at: self.rendered_at,
        })
    }
}

impl AppStateStore for SqliteAppState {
    fn save_live_view(&self, data_root: &Path, record: &LiveViewRecord) -> anyhow::Result<bool> {
        let conn = open_app_db(data_root)?;
        let existed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM live_views WHERE source_kind = ?1 AND source_value = ?2",
            params![record.source_kind, record.source_value],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO live_views (source_kind, source_value, display_name, created_at, last_opened_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source_kind, source_value) DO UPDATE SET display_name = excluded.display_name",
            params![
                record.source_kind,
                record.source_value,
                record.display_name,
                record.created_at,
                record.last_opened_at,
            ],
        )?;
        Ok(existed > 0)
    }

    fn list_live_views(&self, data_root: &Path) -> anyhow::Result<Vec<LiveViewRecord>> {
        let conn = open_app_db(data_root)?;
        let mut stmt = conn.prepare(
            "SELECT source_kind, source_value, display_name, created_at, last_opened_at
             FROM live_views ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(LiveViewRecord {
                source_kind: row.get(0)?,
                source_value: row.get(1)?,
                display_name: row.get(2)?,
                created_at: row.get(3)?,
                last_opened_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn remove_live_view(
        &self,
        data_root: &Path,
        source_kind: &str,
        source_value: &str,
    ) -> anyhow::Result<bool> {
        let conn = open_app_db(data_root)?;
        let removed = conn.execute(
            "DELETE FROM live_views WHERE source_kind = ?1 AND source_value = ?2",
            params![source_kind, source_value],
        )?;
        Ok(removed > 0)
    }

    fn get_setting(&self, data_root: &Path, key: &str) -> anyhow::Result<Option<String>> {
        let conn = open_app_db(data_root)?;
        Ok(conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn set_setting(&self, data_root: &Path, key: &str, value: &str) -> anyhow::Result<()> {
        let conn = open_app_db(data_root)?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    fn record_render(
        &self,
        data_root: &Path,
        record: &NewRecentRenderRecord,
    ) -> anyhow::Result<()> {
        let conn = open_app_db(data_root)?;
        conn.execute(
            "INSERT INTO recent_renders (recipe_json, title, repo_name, kind, range_label, rendered_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.recipe_json,
                record.title,
                record.repo_name,
                record.kind,
                record.range_label,
                record.rendered_at,
            ],
        )?;
        conn.execute(
            "DELETE FROM recent_renders WHERE id NOT IN
             (SELECT id FROM recent_renders ORDER BY id DESC LIMIT ?1)",
            params![i64::try_from(RECENT_RENDERS_CAP)?],
        )?;
        Ok(())
    }

    fn list_recent_renders(
        &self,
        data_root: &Path,
    ) -> Result<Vec<RecentRenderRecord>, AppStateError> {
        let conn = open_app_db(data_root)?;
        let mut stmt = conn
            .prepare(
                "SELECT id, recipe_json, title, repo_name, kind, range_label, rendered_at
                 FROM recent_renders ORDER BY id DESC",
            )
            .map_err(anyhow::Error::from)?;
        let rows = stmt
            .query_map([], RawRecentRenderRecord::from_row)
            .map_err(anyhow::Error::from)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(anyhow::Error::from)?;
        rows.into_iter()
            .map(RawRecentRenderRecord::try_into_record)
            .collect()
    }

    fn get_recent_render(
        &self,
        data_root: &Path,
        id: RenderHistoryId,
    ) -> Result<Option<RecentRenderRecord>, AppStateError> {
        let conn = open_app_db(data_root)?;
        let raw = conn
            .query_row(
                "SELECT id, recipe_json, title, repo_name, kind, range_label, rendered_at
                 FROM recent_renders WHERE id = ?1",
                params![i64::from(id)],
                RawRecentRenderRecord::from_row,
            )
            .optional()
            .map_err(anyhow::Error::from)?;
        raw.map(RawRecentRenderRecord::try_into_record).transpose()
    }
}

#[cfg(test)]
mod tests {
    use application::{
        ports::{
            AppStateError, AppStateStore, LiveViewRecord, NewRecentRenderRecord, RECENT_RENDERS_CAP,
        },
        viewer::RenderHistoryId,
    };

    use super::super::db::open_app_db;
    use crate::app_state::SqliteAppState;

    fn live_view(source_value: &str, display_name: &str, created_at: &str) -> LiveViewRecord {
        LiveViewRecord {
            source_kind: "managed".to_string(),
            source_value: source_value.to_string(),
            display_name: display_name.to_string(),
            created_at: created_at.to_string(),
            last_opened_at: None,
        }
    }

    fn recent_render(title: String) -> NewRecentRenderRecord {
        NewRecentRenderRecord {
            recipe_json: format!(r#"{{"title":"{title}"}}"#),
            title,
            repo_name: "repo".to_string(),
            kind: "range".to_string(),
            range_label: "main..HEAD".to_string(),
            rendered_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn migrations_apply_from_empty_and_land_on_v1() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_app_db(tmp.path()).unwrap();

        let user_version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(user_version, 1);

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
            ("settings", vec!["key", "value"]),
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
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            let columns: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(columns, expected_columns, "columns for {table}");
        }
    }

    #[test]
    fn strict_tables_reject_lossy_types() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_app_db(tmp.path()).unwrap();

        let result = conn.execute("INSERT INTO settings (key, value) VALUES ('k', x'00')", []);
        let error = result.expect_err("STRICT table must reject a BLOB in a TEXT column");
        assert!(
            matches!(
                error,
                rusqlite::Error::SqliteFailure(inner, _)
                    if inner.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_DATATYPE
            ),
            "expected a SqliteFailure with SQLITE_CONSTRAINT_DATATYPE, got: {error}"
        );
    }

    #[test]
    fn connection_pragmas_are_applied() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_app_db(tmp.path()).unwrap();

        let journal_mode: String = conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode, "wal");

        let foreign_keys: i64 = conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
    }

    #[test]
    fn save_then_list_round_trips_a_live_view() {
        let tmp = tempfile::tempdir().unwrap();
        let record = live_view("/repo/one", "One", "2026-01-01T00:00:00Z");

        let existed = SqliteAppState.save_live_view(tmp.path(), &record).unwrap();
        assert!(!existed);

        let listed = SqliteAppState.list_live_views(tmp.path()).unwrap();
        assert_eq!(listed, vec![record]);
    }

    #[test]
    fn saving_the_same_identity_updates_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let first = live_view("/repo/one", "One", "2026-01-01T00:00:00Z");
        SqliteAppState.save_live_view(tmp.path(), &first).unwrap();

        let conn = open_app_db(tmp.path()).unwrap();
        let (rowid, created_at): (i64, String) = conn
            .query_row(
                "SELECT id, created_at FROM live_views WHERE source_value = ?1",
                [&first.source_value],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        drop(conn);

        let second = live_view("/repo/one", "One Renamed", "2026-01-02T00:00:00Z");
        let existed = SqliteAppState.save_live_view(tmp.path(), &second).unwrap();
        assert!(existed);

        let listed = SqliteAppState.list_live_views(tmp.path()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].display_name, "One Renamed");
        assert_eq!(listed[0].created_at, created_at);

        let conn = open_app_db(tmp.path()).unwrap();
        let rowid_after: i64 = conn
            .query_row(
                "SELECT id FROM live_views WHERE source_value = ?1",
                [&first.source_value],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rowid_after, rowid);
    }

    #[test]
    fn remove_live_view_reports_whether_a_row_existed() {
        let tmp = tempfile::tempdir().unwrap();
        let record = live_view("/repo/one", "One", "2026-01-01T00:00:00Z");
        SqliteAppState.save_live_view(tmp.path(), &record).unwrap();

        let removed = SqliteAppState
            .remove_live_view(tmp.path(), &record.source_kind, &record.source_value)
            .unwrap();
        assert!(removed);

        let removed_again = SqliteAppState
            .remove_live_view(tmp.path(), &record.source_kind, &record.source_value)
            .unwrap();
        assert!(!removed_again);
    }

    #[test]
    fn settings_round_trip_and_overwrite() {
        let tmp = tempfile::tempdir().unwrap();

        assert_eq!(
            SqliteAppState.get_setting(tmp.path(), "theme").unwrap(),
            None
        );

        SqliteAppState
            .set_setting(tmp.path(), "theme", "dark")
            .unwrap();
        assert_eq!(
            SqliteAppState.get_setting(tmp.path(), "theme").unwrap(),
            Some("dark".to_string())
        );

        SqliteAppState
            .set_setting(tmp.path(), "theme", "light")
            .unwrap();
        assert_eq!(
            SqliteAppState.get_setting(tmp.path(), "theme").unwrap(),
            Some("light".to_string())
        );
    }

    #[test]
    fn recent_renders_prune_past_the_cap_keeping_newest() {
        let tmp = tempfile::tempdir().unwrap();

        for i in 0..(RECENT_RENDERS_CAP + 5) {
            let mut record = recent_render(format!("render {i}"));
            record.recipe_json = format!("{{\"n\":{i}}}");
            record.rendered_at = format!("2026-01-01T00:{i:02}:00Z");
            SqliteAppState.record_render(tmp.path(), &record).unwrap();
        }

        let listed = SqliteAppState.list_recent_renders(tmp.path()).unwrap();
        assert_eq!(listed.len(), RECENT_RENDERS_CAP);
        assert_eq!(
            listed[0].title,
            format!("render {}", RECENT_RENDERS_CAP + 4)
        );
        assert_eq!(listed[listed.len() - 1].title, "render 5");
    }

    #[test]
    fn recent_render_lookup_round_trips_by_stable_id() {
        let tmp = tempfile::tempdir().unwrap();
        SqliteAppState
            .record_render(tmp.path(), &recent_render("render".into()))
            .unwrap();
        let listed = SqliteAppState.list_recent_renders(tmp.path()).unwrap();
        let id = listed[0].id;

        let found = SqliteAppState
            .get_recent_render(tmp.path(), id)
            .unwrap()
            .expect("record exists");

        assert_eq!(found.id, id);
        assert_eq!(found.title, "render");
    }

    #[test]
    fn recent_render_lookup_returns_none_for_an_absent_id() {
        let tmp = tempfile::tempdir().unwrap();
        let absent_id = RenderHistoryId::try_new(99).expect("positive id");

        let found = SqliteAppState
            .get_recent_render(tmp.path(), absent_id)
            .unwrap();

        assert!(found.is_none());
    }

    #[test]
    fn recent_render_reads_reject_a_non_positive_persisted_id() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_app_db(tmp.path()).unwrap();
        conn.execute(
            "INSERT INTO recent_renders (id, recipe_json, title, repo_name, kind, range_label, rendered_at)
             VALUES (0, '{}', 'invalid', 'repo', 'diff', 'main..HEAD', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        drop(conn);

        let error = SqliteAppState
            .list_recent_renders(tmp.path())
            .expect_err("invalid persisted identity rejects");

        assert!(matches!(
            error,
            AppStateError::InvalidRecentRenderId { id: 0 }
        ));
    }

    #[test]
    fn read_back_type_errors_surface_as_errors_not_panics() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("gtl.db"), "not a database").unwrap();

        let result = SqliteAppState.list_live_views(tmp.path());
        assert!(result.is_err());
    }
}
