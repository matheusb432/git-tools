//! Filesystem adapters for daemon discovery and executable identity.

use std::{
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::Context as _;
use gtl_wire::daemon::{
    DaemonProcessId, ExeIdentity, ExecutableByteLength, ExecutableModifiedUnixMillis, PortFile,
};

/// Returns the daemon discovery path under `store_root`.
pub fn port_file_path(store_root: &Path) -> PathBuf {
    store_root.join("daemon.json")
}

/// Writes a validated daemon discovery record.
///
/// # Errors
///
/// Returns an error when the store directory cannot be created or the record cannot be written.
pub fn write_port_file(store_root: &Path, port_file: PortFile) -> anyhow::Result<()> {
    std::fs::create_dir_all(store_root)?;
    let json = serde_json::to_string(&port_file)?;
    std::fs::write(port_file_path(store_root), json)?;
    Ok(())
}

/// Reads a validated daemon discovery record, returning `None` when absent or invalid.
pub fn read_port_file(store_root: &Path) -> Option<PortFile> {
    let raw = std::fs::read_to_string(port_file_path(store_root)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Removes the discovery record only when it still belongs to `pid`.
pub fn remove_port_file_if_own(store_root: &Path, pid: DaemonProcessId) {
    if read_port_file(store_root).is_some_and(|port_file| port_file.pid == pid) {
        let _ = std::fs::remove_file(port_file_path(store_root));
    }
}

/// Captures the filesystem identity of the executable at `path`.
///
/// # Errors
///
/// Returns an error when metadata cannot produce a `u64` Unix-millisecond identity.
pub fn executable_identity(path: &Path) -> anyhow::Result<ExeIdentity> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("read executable metadata at {}", path.display()))?;
    let modified = metadata
        .modified()
        .with_context(|| format!("read executable modified time at {}", path.display()))?;
    let duration = modified.duration_since(UNIX_EPOCH).with_context(|| {
        format!(
            "executable modified time predates the Unix epoch: {}",
            path.display()
        )
    })?;
    let modified_unix_millis = u64::try_from(duration.as_millis()).with_context(|| {
        format!(
            "executable modified time exceeds u64 milliseconds: {}",
            path.display()
        )
    })?;

    Ok(ExeIdentity::new(
        ExecutableByteLength::new(metadata.len()),
        ExecutableModifiedUnixMillis::new(modified_unix_millis),
    ))
}

#[cfg(test)]
mod tests {
    use gtl_wire::daemon::{DaemonLoopbackPort, DaemonProcessId, PortFile};

    use super::*;

    fn process_id(value: u32) -> DaemonProcessId {
        DaemonProcessId::new(value.try_into().expect("positive fixture process ID"))
    }

    fn port(value: u16) -> DaemonLoopbackPort {
        DaemonLoopbackPort::new(value.try_into().expect("positive fixture loopback port"))
    }

    #[test]
    fn port_file_round_trips_through_the_filesystem_boundary() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let port_file = PortFile {
            port: port(4321),
            pid: process_id(99),
        };

        write_port_file(directory.path(), port_file).expect("write discovery record");

        assert_eq!(read_port_file(directory.path()), Some(port_file));
    }

    #[test]
    fn invalid_port_files_are_not_admitted() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = port_file_path(directory.path());

        for invalid in [
            r#"{"port":0,"pid":99}"#,
            r#"{"port":4321,"pid":0}"#,
            r#"{"port":"4321","pid":99}"#,
            "not JSON",
        ] {
            std::fs::write(&path, invalid).expect("write invalid discovery record");
            assert_eq!(read_port_file(directory.path()), None);
        }
    }

    #[test]
    fn remove_port_file_if_own_preserves_a_successor_record() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let successor = PortFile {
            port: port(4321),
            pid: process_id(42),
        };
        write_port_file(directory.path(), successor).expect("write discovery record");

        remove_port_file_if_own(directory.path(), process_id(7));
        assert_eq!(read_port_file(directory.path()), Some(successor));

        remove_port_file_if_own(directory.path(), successor.pid);
        assert_eq!(read_port_file(directory.path()), None);
    }

    #[test]
    fn executable_identity_projects_filesystem_metadata_once() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("fake-executable");
        std::fs::write(&path, b"0123456789").expect("write fixture executable");

        let identity = executable_identity(&path).expect("read executable identity");

        assert_eq!(identity.length, ExecutableByteLength::new(10));
        assert_eq!(
            identity.modified_unix_millis,
            ExecutableModifiedUnixMillis::new(
                u64::try_from(
                    std::fs::metadata(path)
                        .expect("fixture metadata")
                        .modified()
                        .expect("fixture modified time")
                        .duration_since(UNIX_EPOCH)
                        .expect("fixture modified after epoch")
                        .as_millis()
                )
                .expect("fixture modified time fits u64")
            )
        );
    }
}
