use std::{
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Context as _, Result, ensure};
use clap::Subcommand;
use gtl_infra::app_state::snapshot::{self, SnapshotSchemaNewerError};
use serde::{Deserialize, Serialize};

const SNAPSHOT_FORMAT_VERSION: u32 = 1;
const SNAPSHOT_APPLICATION: &str = "git-tools";
const SNAPSHOT_MANIFEST_FILE_NAME: &str = "manifest.json";
const SNAPSHOT_DATABASE_FILE_NAME: &str = "gtl.db";
const SNAPSHOT_CONFIG_FILE_NAME: &str = "config.toml";
const STAGED_DATABASE_FILE_NAME: &str = "gtl.db.import";
const REFUSED_EXIT_STATUS: u8 = 3;

#[derive(Debug, Subcommand)]
#[command(about = "Selects a data snapshot operation.")]
pub enum DataCommand {
    #[command(
        about = "Write a consistent snapshot of the database and configuration into a new directory."
    )]
    Export {
        #[arg(
            long,
            value_name = "DIR",
            help = "Snapshot directory to create; its parent must exist."
        )]
        to: PathBuf,
    },
    #[command(
        about = "Copy a snapshot beside the live database, migrate it forward, and check its integrity."
    )]
    StageImport {
        #[arg(
            long,
            value_name = "DIR",
            help = "Snapshot directory written by `export`."
        )]
        from: PathBuf,
    },
    #[command(
        about = "Replace the live database with the staged import and restore the snapshot's configuration; the server must be stopped."
    )]
    FinishImport {
        #[arg(
            long,
            value_name = "DIR",
            help = "Snapshot directory passed to `stage-import`."
        )]
        from: PathBuf,
    },
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct RefusedSnapshotError(String);

#[derive(Debug, Deserialize, Serialize)]
struct DataSnapshotManifest {
    format_version: u32,
    application: String,
    schema_version: i64,
}

impl DataSnapshotManifest {
    fn new(schema_version: i64) -> Self {
        Self {
            format_version: SNAPSHOT_FORMAT_VERSION,
            application: SNAPSHOT_APPLICATION.to_owned(),
            schema_version,
        }
    }

    fn read(snapshot_directory: &Path) -> Result<Self> {
        let path = snapshot_directory.join(SNAPSHOT_MANIFEST_FILE_NAME);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading snapshot manifest {}", path.display()))?;
        let manifest: Self = serde_json::from_str(&text)
            .with_context(|| format!("parsing snapshot manifest {}", path.display()))?;
        if manifest.application != SNAPSHOT_APPLICATION {
            return Err(RefusedSnapshotError(format!(
                "snapshot belongs to `{}`, not `{SNAPSHOT_APPLICATION}`",
                manifest.application
            ))
            .into());
        }
        if manifest.format_version != SNAPSHOT_FORMAT_VERSION {
            return Err(RefusedSnapshotError(format!(
                "snapshot format version {} is unsupported; expected {SNAPSHOT_FORMAT_VERSION}",
                manifest.format_version
            ))
            .into());
        }
        Ok(manifest)
    }
}

#[must_use]
pub fn run(command: DataCommand) -> ExitCode {
    let result = gtl_infra::data_root::resolve().and_then(|data_root| match command {
        DataCommand::Export { to } => export(&data_root, &to),
        DataCommand::StageImport { from } => stage_import(&data_root, &from),
        DataCommand::FinishImport { from } => finish_import(&data_root, &from),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gtl-server data: {error:#}");
            if error.downcast_ref::<RefusedSnapshotError>().is_some()
                || error.downcast_ref::<SnapshotSchemaNewerError>().is_some()
            {
                ExitCode::from(REFUSED_EXIT_STATUS)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

fn export(data_root: &Path, snapshot_directory: &Path) -> Result<()> {
    fs::create_dir(snapshot_directory).with_context(|| {
        format!(
            "creating snapshot directory {}",
            snapshot_directory.display()
        )
    })?;
    match write_snapshot(data_root, snapshot_directory) {
        Ok(schema_version) => {
            eprintln!(
                "exported git-tools data at schema version {schema_version} to {}",
                snapshot_directory.display()
            );
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_dir_all(snapshot_directory);
            Err(error)
        }
    }
}

fn write_snapshot(data_root: &Path, snapshot_directory: &Path) -> Result<i64> {
    let schema_version = snapshot::export_database_snapshot(
        data_root,
        &snapshot_directory.join(SNAPSHOT_DATABASE_FILE_NAME),
    )?
    .get();
    if let Some(config_path) = config_path().filter(|path| path.is_file()) {
        fs::copy(
            &config_path,
            snapshot_directory.join(SNAPSHOT_CONFIG_FILE_NAME),
        )
        .with_context(|| format!("copying git-tools configuration {}", config_path.display()))?;
    }
    let manifest = serde_json::to_string_pretty(&DataSnapshotManifest::new(schema_version))
        .context("serializing snapshot manifest")?;
    fs::write(
        snapshot_directory.join(SNAPSHOT_MANIFEST_FILE_NAME),
        manifest + "\n",
    )
    .context("writing snapshot manifest")?;
    Ok(schema_version)
}

fn stage_import(data_root: &Path, snapshot_directory: &Path) -> Result<()> {
    DataSnapshotManifest::read(snapshot_directory)?;
    fs::create_dir_all(data_root)
        .with_context(|| format!("creating git-tools data directory {}", data_root.display()))?;

    let staged_path = data_root.join(STAGED_DATABASE_FILE_NAME);
    remove_file_if_present(&staged_path)?;
    match snapshot::stage_database_snapshot(
        &snapshot_directory.join(SNAPSHOT_DATABASE_FILE_NAME),
        &staged_path,
    ) {
        Ok(staged) => {
            eprintln!(
                "staged git-tools data from {} and migrated schema version {} to {}",
                snapshot_directory.display(),
                staged.snapshot_schema_version.get(),
                staged.schema_version.get()
            );
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&staged_path);
            Err(error)
        }
    }
}

fn finish_import(data_root: &Path, snapshot_directory: &Path) -> Result<()> {
    let staged_path = data_root.join(STAGED_DATABASE_FILE_NAME);
    ensure!(
        staged_path.is_file(),
        "no staged git-tools import at {}",
        staged_path.display()
    );
    let endpoint = gtl_local_transport::LocalEndpoint::from_root(data_root)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    // Owning the endpoint excludes both a live server and a concurrent restart during the swap.
    let _listener = runtime
        .block_on(async {
            gtl_local_transport::LocalListener::bind(&endpoint, std::time::Duration::from_secs(1))
                .await
        })
        .map_err(|source| {
            anyhow::Error::new(RefusedSnapshotError(
                "stop gtl-server before importing data; its local endpoint is unavailable".into(),
            ))
            .context(source)
        })?;
    snapshot::replace_database_file(&staged_path, data_root)?;
    restore_configuration(snapshot_directory)
}

fn restore_configuration(snapshot_directory: &Path) -> Result<()> {
    let snapshot_config_path = snapshot_directory.join(SNAPSHOT_CONFIG_FILE_NAME);
    if !snapshot_config_path.is_file() {
        return Ok(());
    }
    let config_path =
        config_path().context("resolving the git-tools configuration path to restore")?;
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating configuration directory {}", parent.display()))?;
    }
    fs::copy(&snapshot_config_path, &config_path)
        .with_context(|| format!("writing git-tools configuration {}", config_path.display()))?;
    Ok(())
}

fn config_path() -> Option<PathBuf> {
    gtl_infra::user_config::TomlSettingsStore::from_environment()
        .path()
        .map(Path::to_path_buf)
}

fn remove_file_if_present(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("removing stale staged import {}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_preserves_every_database_file_while_the_server_owns_the_endpoint() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let endpoint = gtl_local_transport::LocalEndpoint::from_root(&root).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _listener = runtime
            .block_on(async {
                gtl_local_transport::LocalListener::bind(
                    &endpoint,
                    std::time::Duration::from_secs(1),
                )
                .await
            })
            .unwrap();
        for name in [
            "gtl.db",
            "gtl.db-wal",
            "gtl.db-shm",
            STAGED_DATABASE_FILE_NAME,
        ] {
            fs::write(root.join(name), name).unwrap();
        }

        let error = finish_import(&root, &root).unwrap_err();

        assert!(
            error.downcast_ref::<RefusedSnapshotError>().is_some(),
            "{error:#}"
        );
        for name in [
            "gtl.db",
            "gtl.db-wal",
            "gtl.db-shm",
            STAGED_DATABASE_FILE_NAME,
        ] {
            assert_eq!(fs::read(root.join(name)).unwrap(), name.as_bytes());
        }
    }

    #[test]
    fn import_replaces_the_database_after_the_endpoint_is_released() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        fs::write(root.join("gtl.db"), b"old database").unwrap();
        fs::write(root.join("gtl.db-wal"), b"old WAL").unwrap();
        fs::write(root.join(STAGED_DATABASE_FILE_NAME), b"imported database").unwrap();

        finish_import(&root, &root).unwrap();

        assert_eq!(fs::read(root.join("gtl.db")).unwrap(), b"imported database");
        assert!(!root.join("gtl.db-wal").exists());
        assert!(!root.join(STAGED_DATABASE_FILE_NAME).exists());
    }
}
