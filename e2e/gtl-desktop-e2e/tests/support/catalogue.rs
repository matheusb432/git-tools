use std::path::Path;

use anyhow::{Context as _, Result};
use gtl_infra::app_state::SqliteAppState;
use rusqlite::params;

pub struct ProjectCatalogue {
    database: SqliteAppState,
}

impl ProjectCatalogue {
    pub fn open(data_root: &Path) -> Result<Self> {
        Ok(Self {
            database: SqliteAppState::open(data_root)?,
        })
    }

    pub fn make_unavailable(&self) -> Result<()> {
        self.database
            .connection_lock()?
            .execute_batch("ALTER TABLE projects RENAME TO unavailable_projects")?;
        Ok(())
    }

    pub fn seed_project_snapshots(&self, project_id: &str, count: i64) -> Result<()> {
        let mut connection = self.database.connection_lock()?;
        let transaction = connection.transaction()?;
        let existing: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM recent_renders WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )?;
        for index in existing..count {
            transaction.execute(
                "INSERT INTO recent_renders (source_id, operation_id, target_id, argument, pinned_base, pinned_head, recipe_name, title, repo_name, range_label, rendered_at, project_id)
                 SELECT source_id, operation_id, target_id, argument, pinned_base, ?3, ?2, ?2, repo_name, range_label, rendered_at, project_id
                 FROM recent_renders WHERE project_id = ?1 AND pinned_base IS NOT NULL ORDER BY id LIMIT 1",
                params![
                    project_id,
                    format!("Snapshot {index:02}"),
                    format!("{index:040x}")
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn set_projects(&self, projects: &[(&str, &str, &Path)]) -> Result<()> {
        let fixture_home = std::env::var_os("HOME").context("isolated fixture home")?;
        let mut connection = self.database.connection_lock()?;
        let transaction = connection.transaction()?;
        transaction.execute_batch("DELETE FROM projects; DELETE FROM project_sources;")?;
        for (id, title, path) in projects {
            let relative = path
                .strip_prefix(&fixture_home)
                .context("fixture projects under home")?;
            transaction.execute(
                "INSERT INTO project_sources (source_kind, source_value) VALUES ('directory', ?1)",
                [format!(
                    "~/{}",
                    relative.to_string_lossy().replace('\\', "/")
                )],
            )?;
            let source_id = transaction.last_insert_rowid();
            transaction.execute("INSERT INTO projects (id, source_id, title, mux_session_name, affiliation, export_include_in_all) VALUES (?1, ?2, ?3, ?4, 'personal', 1)", params![id, source_id, title, id.to_ascii_lowercase()])?;
        }
        transaction.commit()?;
        Ok(())
    }
}
