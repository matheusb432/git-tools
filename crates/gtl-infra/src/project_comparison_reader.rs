use gtl_application::{ports::ProjectComparisonReader, projects::get_project_comparison_branch};
use gtl_models::{paths::RepositoryRoot, projects::comparison::ComparisonBranch};
use rusqlite::Connection;

use crate::app_state::SqliteAppState;

impl ProjectComparisonReader for SqliteAppState {
    fn comparison_branch(&self, path: &RepositoryRoot) -> anyhow::Result<Option<ComparisonBranch>> {
        let Some(source) = path.as_ref().to_str() else {
            return Ok(None);
        };
        let Ok(source) = gtl_models::projects::catalogue::ProjectDirectorySource::try_new(source)
        else {
            return Ok(None);
        };
        let connection = self.connection_lock()?;
        match get_project_comparison_branch::execute(&source, &connection) {
            Ok(branch) => Ok(branch),
            Err(_) if !catalogue_tables_available(&connection)? => Ok(None),
            Err(error) => Err(error),
        }
    }
}

fn catalogue_tables_available(connection: &Connection) -> rusqlite::Result<bool> {
    let count: i64 = connection.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name IN ('projects', 'project_sources')",
        [],
        |row| row.get(0),
    )?;
    Ok(count == 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_repository_uses_default_when_catalogue_tables_are_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let database = SqliteAppState::open(directory.path()).unwrap();
        let repository =
            RepositoryRoot::try_new(directory.path().join("outside-home-repo")).unwrap();
        {
            let connection = database.connection_lock().unwrap();
            connection
                .execute(
                    "INSERT INTO project_sources (source_kind, source_value) VALUES ('directory', ?1)",
                    [repository.to_string()],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO projects (id, source_id, title, comparison_branch) VALUES ('OHR', last_insert_rowid(), 'Outside home', 'develop')",
                    [],
                )
                .unwrap();
        }
        let saved = database.comparison_branch(&repository).unwrap();
        assert_eq!(saved.unwrap().to_string(), "develop");

        database
            .connection_lock()
            .unwrap()
            .execute_batch("ALTER TABLE projects RENAME TO unavailable_projects")
            .unwrap();
        assert!(database.comparison_branch(&repository).unwrap().is_none());
    }
}
