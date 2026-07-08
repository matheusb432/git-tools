//! The `live_views/remove` vertical slice: delete a saved live view by identity.

use std::path::PathBuf;

use cqrsy::Handler;

use crate::ports::AppStateStore;

/// Delete the saved live view identified by `(source_kind, source_value)`.
#[derive(Debug, Clone, PartialEq, cqrsy::Command)]
#[command(out = RemoveLiveViewResponse, err = RemoveLiveViewError)]
pub struct RemoveLiveView {
    pub data_root: PathBuf,
    pub source_kind: String,
    pub source_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveLiveViewResponse {
    pub removed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RemoveLiveViewError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Handles [`RemoveLiveView`] by deleting through the app-state port.
#[derive(Clone)]
pub struct RemoveLiveViewHandler<A: AppStateStore> {
    pub store: A,
}

impl<A: AppStateStore> Handler<RemoveLiveView> for RemoveLiveViewHandler<A> {
    async fn handle(
        &self,
        req: RemoveLiveView,
    ) -> Result<RemoveLiveViewResponse, RemoveLiveViewError> {
        let removed =
            self.store
                .remove_live_view(&req.data_root, &req.source_kind, &req.source_value)?;
        Ok(RemoveLiveViewResponse { removed })
    }
}

#[cfg(test)]
mod tests {
    use cqrsy::send_now;

    use super::*;
    use crate::{ports::LiveViewRecord, testing::InMemoryAppStateStore};

    #[test]
    fn remove_reports_true_when_a_row_was_removed() {
        let store = InMemoryAppStateStore::default();
        store.live_views.lock().unwrap().push(LiveViewRecord {
            source_kind: "LocalRepo".into(),
            source_value: "/repos/gt".into(),
            display_name: "gt".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            last_opened_at: None,
        });
        let handler = RemoveLiveViewHandler {
            store: store.clone(),
        };

        let response = send_now(
            &(),
            &handler,
            RemoveLiveView {
                data_root: "/data".into(),
                source_kind: "LocalRepo".into(),
                source_value: "/repos/gt".into(),
            },
        )
        .expect("remove succeeds");

        assert!(response.removed);
        assert!(store.live_views.lock().unwrap().is_empty());
    }

    #[test]
    fn remove_reports_false_for_unknown_identity() {
        let handler = RemoveLiveViewHandler {
            store: InMemoryAppStateStore::default(),
        };

        let response = send_now(
            &(),
            &handler,
            RemoveLiveView {
                data_root: "/data".into(),
                source_kind: "LocalRepo".into(),
                source_value: "/repos/unknown".into(),
            },
        )
        .expect("remove succeeds");

        assert!(!response.removed);
    }
}
