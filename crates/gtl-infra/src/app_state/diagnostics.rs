//! Read-only database inspection; startup alone creates and migrates state.

use std::{path::Path, time::Duration};

use anyhow::{Context as _, ensure};
use rusqlite::{Connection, OpenFlags};

use super::db::{DATABASE_FILE_NAME, known_schema_version};

#[derive(Debug, PartialEq, Eq)]
pub enum DatabaseInspection {
    Missing,
    Current,
    Pending { count: usize },
}

pub fn inspect_database(data_root: &Path) -> anyhow::Result<DatabaseInspection> {
    let path = data_root.join(DATABASE_FILE_NAME);
    if !path.try_exists()? {
        return Ok(DatabaseInspection::Missing);
    }
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening database {} read-only", path.display()))?;
    connection.busy_timeout(Duration::from_secs(3))?;
    let integrity: String = connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    ensure!(
        integrity == "ok",
        "database integrity check failed: {integrity}"
    );
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let version = usize::try_from(version).context("database has a negative schema version")?;
    let expected = known_schema_version();
    ensure!(
        version <= expected,
        "database schema {version} is newer than this server's schema {expected}; install a compatible release"
    );
    Ok(if version == expected {
        DatabaseInspection::Current
    } else {
        DatabaseInspection::Pending {
            count: expected - version,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::SqliteAppState;

    #[test]
    fn inspection_does_not_create_or_migrate_database() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("missing");
        assert_eq!(
            inspect_database(&root).unwrap(),
            DatabaseInspection::Missing
        );
        assert!(!root.exists());
        std::fs::create_dir(&root).unwrap();
        let path = root.join(DATABASE_FILE_NAME);
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        drop(connection);
        let before = std::fs::read(&path).unwrap();
        assert!(matches!(
            inspect_database(&root).unwrap(),
            DatabaseInspection::Pending { .. }
        ));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn inspection_accepts_current_and_rejects_newer_or_corrupt_database() {
        let directory = tempfile::tempdir().unwrap();
        let state = SqliteAppState::open(directory.path()).unwrap();
        assert_eq!(
            inspect_database(directory.path()).unwrap(),
            DatabaseInspection::Current
        );
        state
            .connection_lock()
            .unwrap()
            .pragma_update(None, "user_version", 1000)
            .unwrap();
        assert!(
            inspect_database(directory.path())
                .unwrap_err()
                .to_string()
                .contains("newer")
        );
        drop(state);
        std::fs::write(directory.path().join(DATABASE_FILE_NAME), "corrupt").unwrap();
        assert!(inspect_database(directory.path()).is_err());
    }
}
