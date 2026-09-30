use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, Result, ensure};
use rusqlite::{Connection, OpenFlags};

use super::db::{DATABASE_FILE_NAME, MIGRATIONS, known_schema_version};

const SNAPSHOT_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const SQLITE_COMPANION_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];

#[derive(Debug, thiserror::Error)]
#[error("snapshot schema is newer than this build; upgrade before importing")]
pub struct SnapshotSchemaNewerError;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DatabaseSchemaVersion(i64);

impl DatabaseSchemaVersion {
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct StagedDatabaseSnapshot {
    pub snapshot_schema_version: DatabaseSchemaVersion,
    pub schema_version: DatabaseSchemaVersion,
}

pub fn export_database_snapshot(
    data_root: &Path,
    snapshot_path: &Path,
) -> Result<DatabaseSchemaVersion> {
    let database_path = data_root.join(DATABASE_FILE_NAME);
    ensure!(
        database_path.is_file(),
        "app-state database does not exist: {}",
        database_path.display()
    );
    let snapshot_path_text = snapshot_path.to_str().with_context(|| {
        format!(
            "snapshot path is not valid UTF-8: {}",
            snapshot_path.display()
        )
    })?;

    let live = Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| {
            format!(
                "opening live app-state database {}",
                database_path.display()
            )
        })?;
    live.busy_timeout(SNAPSHOT_BUSY_TIMEOUT)?;
    live.execute("VACUUM INTO ?1", [snapshot_path_text])
        .with_context(|| {
            format!(
                "writing app-state database snapshot {}",
                snapshot_path.display()
            )
        })?;
    drop(live);

    let snapshot = Connection::open(snapshot_path)
        .with_context(|| format!("opening written snapshot {}", snapshot_path.display()))?;
    snapshot.pragma_update(None, "journal_mode", "DELETE")?;
    let schema_version: i64 =
        snapshot.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(DatabaseSchemaVersion(schema_version))
}

pub fn stage_database_snapshot(
    snapshot_path: &Path,
    staged_path: &Path,
) -> Result<StagedDatabaseSnapshot> {
    copy_file_new(snapshot_path, staged_path).with_context(|| {
        format!(
            "copying app-state database snapshot to {}",
            staged_path.display()
        )
    })?;

    let mut staged = Connection::open(staged_path).with_context(|| {
        format!(
            "opening staged app-state database {}",
            staged_path.display()
        )
    })?;
    staged.busy_timeout(SNAPSHOT_BUSY_TIMEOUT)?;
    staged.pragma_update(None, "foreign_keys", true)?;

    let snapshot_version: i64 =
        staged.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let known_version =
        i64::try_from(known_schema_version()).context("known schema version overflows i64")?;
    if snapshot_version < 0 || snapshot_version > known_version {
        return Err(SnapshotSchemaNewerError.into());
    }

    MIGRATIONS.to_latest(&mut staged).with_context(|| {
        format!(
            "migrating staged app-state database {}",
            staged_path.display()
        )
    })?;

    let report: String = staged.pragma_query_value(None, "integrity_check", |row| row.get(0))?;
    ensure!(
        report == "ok",
        "staged app-state database failed its integrity check: {report}"
    );

    Ok(StagedDatabaseSnapshot {
        snapshot_schema_version: DatabaseSchemaVersion(snapshot_version),
        schema_version: DatabaseSchemaVersion(known_version),
    })
}

pub fn replace_database_file(staged_path: &Path, data_root: &Path) -> Result<()> {
    let database_path = data_root.join(DATABASE_FILE_NAME);
    for suffix in SQLITE_COMPANION_SUFFIXES {
        let companion = companion_path(&database_path, suffix);
        match fs::remove_file(&companion) {
            Ok(()) => {}
            Err(source) if source.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(source).with_context(|| {
                    format!("removing stale SQLite companion {}", companion.display())
                });
            }
        }
    }
    fs::rename(staged_path, &database_path).with_context(|| {
        format!(
            "moving staged app-state database into place at {}",
            database_path.display()
        )
    })
}

fn copy_file_new(source_path: &Path, destination_path: &Path) -> io::Result<()> {
    let mut source = File::open(source_path)?;
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination_path)?;
    io::copy(&mut source, &mut destination)?;
    destination.sync_all()
}

fn companion_path(database_path: &Path, suffix: &str) -> PathBuf {
    let mut path = database_path.as_os_str().to_os_string();
    path.push(suffix);
    path.into()
}
