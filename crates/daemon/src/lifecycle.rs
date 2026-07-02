//! Daemon identity and discovery: the port file and the exe identity used by
//! the client's version handshake (restart-on-mismatch after `just update`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The discovery record written under the data dir: where the daemon listens
/// and which process owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortFile {
    pub port: u16,
    pub pid: u32,
}

/// The port file path for a given store root: `<store_root>/daemon.json`.
pub fn port_file_path(store_root: &Path) -> PathBuf {
    store_root.join("daemon.json")
}

/// Write the port file, creating `store_root` if needed.
///
/// # Errors
/// Returns an error if the directory cannot be created or the file cannot be written.
pub fn write_port_file(store_root: &Path, entry: &PortFile) -> anyhow::Result<()> {
    std::fs::create_dir_all(store_root)?;
    let json = serde_json::to_string(entry)?;
    std::fs::write(port_file_path(store_root), json)?;
    Ok(())
}

/// Read and parse the port file, returning `None` when it is absent or malformed.
pub fn read_port_file(store_root: &Path) -> Option<PortFile> {
    let raw = std::fs::read_to_string(port_file_path(store_root)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Remove the port file only when it still names `pid` (never clobber a
/// successor daemon's file).
pub fn remove_port_file_if_own(store_root: &Path, pid: u32) {
    if read_port_file(store_root).is_some_and(|f| f.pid == pid) {
        let _ = std::fs::remove_file(port_file_path(store_root));
    }
}

/// Size + mtime of an executable — cheap content identity for the handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExeIdentity {
    pub exe_len: u64,
    pub exe_modified_ms: u64,
}

impl ExeIdentity {
    /// Capture the identity of the file at `path` from its filesystem metadata.
    ///
    /// # Errors
    /// Returns an error if the metadata (length, mtime) cannot be read.
    pub fn of(path: &Path) -> anyhow::Result<Self> {
        let meta = std::fs::metadata(path)?;
        let modified = meta.modified()?;
        let ms = modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        Ok(Self {
            exe_len: meta.len(),
            exe_modified_ms: ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_file_round_trips_through_a_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let entry = PortFile {
            port: 4321,
            pid: 99,
        };
        write_port_file(dir.path(), &entry).unwrap();
        assert_eq!(read_port_file(dir.path()), Some(entry));
    }

    #[test]
    fn read_port_file_is_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_port_file(dir.path()), None);
    }

    #[test]
    fn remove_port_file_if_own_leaves_a_file_naming_a_different_pid() {
        let dir = tempfile::tempdir().unwrap();
        write_port_file(dir.path(), &PortFile { port: 1, pid: 42 }).unwrap();

        // A different pid must not remove another daemon's file.
        remove_port_file_if_own(dir.path(), 7);
        assert_eq!(
            read_port_file(dir.path()),
            Some(PortFile { port: 1, pid: 42 })
        );

        // The owning pid removes it.
        remove_port_file_if_own(dir.path(), 42);
        assert_eq!(read_port_file(dir.path()), None);
    }

    #[test]
    fn exe_identity_reports_the_file_length() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake-exe");
        std::fs::write(&path, b"0123456789").unwrap();
        let identity = ExeIdentity::of(&path).unwrap();
        assert_eq!(identity.exe_len, 10);
    }
}
