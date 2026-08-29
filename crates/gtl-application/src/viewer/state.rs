//! Shared process-local viewer state and version notifications.

use std::sync::{Arc, Mutex};

use gtl_models::viewer::ViewerVersion;

use super::session::{DEFAULT_VIEW_CACHE_WEIGHT, ViewerSession};

#[derive(Clone)]
pub struct ViewerState {
    session: Arc<Mutex<ViewerSession>>,
    version_sender: tokio::sync::watch::Sender<ViewerVersion>,
}

#[derive(Debug, thiserror::Error)]
pub enum ViewerStateError {
    #[error("viewer state lock is poisoned")]
    LockPoisoned,
}

impl ViewerState {
    /// Creates an empty per-server viewer session with the default cache bound.
    #[must_use]
    pub fn new() -> Self {
        let version = ViewerVersion::default();
        let (version_sender, _) = tokio::sync::watch::channel(version);
        Self {
            session: Arc::new(Mutex::new(ViewerSession::new(DEFAULT_VIEW_CACHE_WEIGHT))),
            version_sender,
        }
    }

    /// Runs a read that may update cache recency but does not change visible shell output.
    pub fn inspect<Output>(
        &self,
        inspect: impl FnOnce(&mut ViewerSession) -> Output,
    ) -> Result<Output, ViewerStateError> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| ViewerStateError::LockPoisoned)?;
        Ok(inspect(&mut session))
    }

    /// Applies one short state change and notifies watchers when its visible version advances.
    pub fn update<Output>(
        &self,
        update: impl FnOnce(&mut ViewerSession) -> Output,
    ) -> Result<Output, ViewerStateError> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| ViewerStateError::LockPoisoned)?;
        let before = session.version();
        let output = update(&mut session);
        let after = session.version();
        drop(session);
        if after != before {
            self.version_sender.send_replace(after);
        }
        Ok(output)
    }

    /// Marks a shell projection change that is not otherwise stored in the session, such as theme.
    pub fn mark_shell_changed(&self) -> Result<ViewerVersion, ViewerStateError> {
        self.update(|session| {
            session.mark_shell_changed();
            session.version()
        })
    }

    /// Returns the current version without changing session state.
    pub fn version(&self) -> Result<ViewerVersion, ViewerStateError> {
        self.inspect(|session| session.version())
    }

    /// Subscribes to the current version and every later visible state change.
    #[must_use]
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<ViewerVersion> {
        self.version_sender.subscribe()
    }
}

impl Default for ViewerState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watchers_receive_only_visible_version_changes() {
        let state = ViewerState::new();
        let mut receiver = state.subscribe();

        assert_eq!(*receiver.borrow(), ViewerVersion::default());
        state.inspect(|_| ()).unwrap();
        assert!(!receiver.has_changed().unwrap());

        let version = state.mark_shell_changed().unwrap();

        assert_eq!(version, ViewerVersion::new(1));
        assert!(receiver.has_changed().unwrap());
        assert_eq!(*receiver.borrow_and_update(), version);
    }
}
