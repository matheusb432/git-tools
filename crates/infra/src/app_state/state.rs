use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use application::ports::AppStateStore;
use parking_lot::{Mutex, MutexGuard};
use rusqlite::Connection;

use super::db::open_app_db;

const CONNECTION_LOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// Owns one initialized app-state connection shared by every clone.
#[derive(Debug, Clone)]
pub struct SqliteAppState {
    inner: Arc<SqliteAppStateInner>,
}

#[derive(Debug)]
struct SqliteAppStateInner {
    connection: Mutex<Connection>,
    data_root: PathBuf,
}

impl SqliteAppState {
    /// Opens and initializes the app-state database under `data_root`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn open() -> anyhow::Result<()> {
    /// let state = infra::app_state::SqliteAppState::open(std::path::Path::new("/var/lib/gtl"))?;
    /// let state_clone = state.clone();
    /// # drop(state_clone);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error when the directory, database, pragmas, or migrations fail.
    pub fn open(data_root: &Path) -> anyhow::Result<Self> {
        let connection = open_app_db(data_root)?;
        Ok(Self {
            inner: Arc::new(SqliteAppStateInner {
                connection: Mutex::new(connection),
                data_root: data_root.to_path_buf(),
            }),
        })
    }

    fn connection_lock_timeout(
        &self,
        timeout: Duration,
    ) -> anyhow::Result<MutexGuard<'_, Connection>> {
        self.inner
            .connection
            .try_lock_for(timeout)
            .with_context(|| {
                format!(
                    "timed out after {} ms locking app-state connection for {}",
                    timeout.as_millis(),
                    self.inner.data_root.display()
                )
            })
    }
}

impl AppStateStore for SqliteAppState {
    fn connection_lock(&self) -> anyhow::Result<impl std::ops::DerefMut<Target = Connection> + '_> {
        self.connection_lock_timeout(CONNECTION_LOCK_TIMEOUT)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use application::ports::AppStateStore;

    use super::SqliteAppState;

    #[test]
    fn clone_observes_connection_local_state_from_original() {
        let directory = tempfile::tempdir().expect("temporary data root");
        let state = SqliteAppState::open(directory.path()).expect("open app state");
        let state_clone = state.clone();
        state
            .connection_lock()
            .expect("connection lock")
            .execute_batch(
                "CREATE TEMP TABLE connection_local (value TEXT NOT NULL); \
                 INSERT INTO connection_local (value) VALUES ('shared');",
            )
            .expect("create connection-local state");

        let count: i64 = state_clone
            .connection_lock()
            .expect("clone connection lock")
            .query_row("SELECT COUNT(*) FROM connection_local", [], |row| {
                row.get(0)
            })
            .expect("read connection-local state");

        assert_eq!(count, 1);
    }

    #[test]
    fn connection_lock_timeout_is_bounded_and_contextual() {
        let directory = tempfile::tempdir().expect("temporary data root");
        let state = SqliteAppState::open(directory.path()).expect("open app state");
        let _guard = state.connection_lock().expect("connection lock");

        let error = state
            .connection_lock_timeout(Duration::from_millis(10))
            .expect_err("second lock times out");

        assert!(
            error
                .to_string()
                .contains(&directory.path().display().to_string())
        );
    }

    #[test]
    fn app_state_is_clone_send_sync_static() {
        fn assert_bounds<T: Clone + Send + Sync + 'static>() {}

        assert_bounds::<SqliteAppState>();
    }
}
