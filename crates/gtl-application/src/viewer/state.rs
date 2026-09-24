//! Shared process-local viewer state and version notifications.

use std::sync::{Arc, Mutex};

use gtl_models::{failure::ErrorMeta, viewer::ViewerVersion};

use super::{
    ViewerDiffSnapshot,
    session::{DEFAULT_VIEW_CACHE_WEIGHT, ViewerSession},
};
use crate::diffs::{View, source_lines::DiffSourcePool};

#[derive(Clone)]
pub struct ViewerState {
    session: Arc<Mutex<ViewerSession>>,
    sources: Arc<Mutex<DiffSourcePool>>,
    version_sender: tokio::sync::watch::Sender<ViewerVersion>,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ViewerStateError {
    #[error("viewer state lock is poisoned")]
    #[meta(private(Internal))]
    LockPoisoned,
    #[error("viewer source pool lock is poisoned")]
    #[meta(private(Internal))]
    SourceLockPoisoned,
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
            sources: Arc::default(),
        }
    }

    /// Shares immutable sources and computes digests outside the session lock.
    pub(super) fn prepare_snapshot(
        &self,
        mut view: Arc<View>,
    ) -> Result<ViewerDiffSnapshot, ViewerStateError> {
        let mut sources = self
            .sources
            .lock()
            .map_err(|_| ViewerStateError::SourceLockPoisoned)?;
        for file in &mut Arc::make_mut(&mut view).files {
            sources.intern(&mut file.lines);
            if let Some(full) = &mut file.full_lines {
                sources.intern(full);
            }
        }
        drop(sources);
        Ok(ViewerDiffSnapshot::new(view))
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
    fn snapshots_share_equal_compact_and_full_sources_but_keep_view_metadata() {
        let state = ViewerState::new();
        let make_view = |title: &str| {
            let mut view = crate::utils::diffs::view();
            view.title = title.to_owned();
            view.files = vec![crate::diffs::FileDiff {
                path: crate::utils::repository_relative_path("f.txt"),
                added: gtl_models::diffs::DiffLineCount::new(1),
                removed: gtl_models::diffs::DiffLineCount::default(),
                lines: ["@@ -0,0 +1 @@", "+same"].into_iter().collect(),
                full_lines: None,
            }];
            view.files[0].full_lines = Some(view.files[0].lines.iter().collect());
            Arc::new(view)
        };
        let first = state.prepare_snapshot(make_view("live")).unwrap();
        let second = state.prepare_snapshot(make_view("snapshot")).unwrap();
        let first_text = first.files[0].lines.iter().next().unwrap();
        assert_eq!(
            first_text.as_ptr(),
            second.files[0].lines.iter().next().unwrap().as_ptr()
        );
        assert_eq!(
            first_text.as_ptr(),
            first.files[0]
                .full_lines
                .as_ref()
                .unwrap()
                .iter()
                .next()
                .unwrap()
                .as_ptr()
        );
        assert_eq!(first.title, "live");
        assert_eq!(second.title, "snapshot");
    }

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
