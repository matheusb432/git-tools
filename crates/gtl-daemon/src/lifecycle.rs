//! Daemon ownership locks for startup election and process lifetime.

use std::{
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Context as _;

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

#[cfg(test)]
mod tests {
    use super::*;

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
