//! Daemon ownership, identity, and discovery: the process lock, port file, and
//! executable identity used by the client's version handshake.

use std::{
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

/// The held daemon ownership lock. Dropping it releases ownership.
pub(crate) struct DaemonLock {
    file: File,
}

struct DaemonLockStartup {
    file: File,
}

impl Drop for DaemonLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

impl Drop for DaemonLockStartup {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn daemon_lock_path(store_root: &Path) -> PathBuf {
    store_root.join("daemon.lock")
}

fn daemon_lock_path_startup(store_root: &Path) -> PathBuf {
    store_root.join("daemon.start.lock")
}

fn try_acquire_daemon_lock_startup(store_root: &Path) -> anyhow::Result<Option<DaemonLockStartup>> {
    std::fs::create_dir_all(store_root)?;
    let path = daemon_lock_path_startup(store_root);
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("open daemon startup lock at {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(DaemonLockStartup { file })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error)
            .with_context(|| format!("lock daemon startup election at {}", path.display())),
    }
}

/// Elects one starter, then waits within `budget` for transient ownership probes.
pub(crate) fn acquire_daemon_lock(
    store_root: &Path,
    budget: Duration,
) -> anyhow::Result<Option<DaemonLock>> {
    let Some(_startup_lock) = try_acquire_daemon_lock_startup(store_root)? else {
        return Ok(None);
    };
    let path = daemon_lock_path(store_root);
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("open daemon lock at {}", path.display()))?;
    match file.try_lock_shared() {
        Ok(()) => file
            .unlock()
            .with_context(|| format!("unlock daemon ownership probe at {}", path.display()))?,
        Err(TryLockError::WouldBlock) => return Ok(None),
        Err(TryLockError::Error(error)) => {
            return Err(error)
                .with_context(|| format!("inspect daemon ownership at {}", path.display()));
        }
    }

    // The startup election excludes daemon contenders; only shared client probes can block here.
    let deadline = Instant::now() + budget;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Some(DaemonLock { file })),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => {
                return Err(error)
                    .with_context(|| format!("lock daemon ownership at {}", path.display()));
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        std::thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

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
pub fn write_port_file(store_root: &Path, entry: PortFile) -> anyhow::Result<()> {
    std::fs::create_dir_all(store_root)?;
    let json = serde_json::to_string(&entry)?;
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
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
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
        write_port_file(dir.path(), entry).unwrap();
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
        write_port_file(dir.path(), PortFile { port: 1, pid: 42 }).unwrap();

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

    #[test]
    fn daemon_lock_has_one_owner_and_is_reacquirable_after_release() {
        let directory = tempfile::tempdir().unwrap();

        let first = acquire_daemon_lock(directory.path(), std::time::Duration::ZERO)
            .unwrap()
            .expect("first lock owner");
        assert!(
            acquire_daemon_lock(directory.path(), std::time::Duration::ZERO)
                .unwrap()
                .is_none(),
            "a second handle must not acquire an owned lock"
        );

        drop(first);
        assert!(
            acquire_daemon_lock(directory.path(), std::time::Duration::ZERO)
                .unwrap()
                .is_some(),
            "the lock must be available after its owner drops"
        );
        assert!(directory.path().join("daemon.lock").is_file());
    }

    #[test]
    fn daemon_lock_acquisition_yields_to_an_elected_startup() {
        let directory = tempfile::tempdir().unwrap();
        let path = daemon_lock_path_startup(directory.path());
        let startup_owner = OpenOptions::new()
            .create(true)
            .read(true)
            .truncate(false)
            .write(true)
            .open(path)
            .unwrap();
        startup_owner.try_lock().unwrap();

        let acquired =
            acquire_daemon_lock(directory.path(), std::time::Duration::from_millis(500)).unwrap();

        assert!(acquired.is_none());
    }

    #[test]
    fn daemon_lock_acquisition_waits_for_a_transient_probe_owner() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("daemon.lock");
        let transient = OpenOptions::new()
            .create(true)
            .read(true)
            .truncate(false)
            .write(true)
            .open(path)
            .unwrap();
        transient.lock_shared().unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(25));
            drop(transient);
        });

        let acquired =
            acquire_daemon_lock(directory.path(), std::time::Duration::from_millis(250)).unwrap();

        release.join().unwrap();
        assert!(acquired.is_some());
    }

    #[test]
    fn daemon_lock_acquisition_yields_immediately_to_an_exclusive_owner() {
        let directory = tempfile::tempdir().unwrap();
        let _owner = acquire_daemon_lock(directory.path(), std::time::Duration::ZERO)
            .unwrap()
            .expect("exclusive daemon owner");
        let started_at = std::time::Instant::now();

        let acquired =
            acquire_daemon_lock(directory.path(), std::time::Duration::from_millis(500)).unwrap();

        assert!(acquired.is_none());
        assert!(started_at.elapsed() < std::time::Duration::from_millis(250));
    }
}
