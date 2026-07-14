//! The `live_views/remove` vertical slice: delete a saved live view by identity.

use std::path::PathBuf;

use crate::ports::AppStateStore;

/// Delete the saved live view identified by `(source_kind, source_value)`.
#[derive(Debug, Clone, PartialEq)]
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

/// Removes a saved live view through the app-state port.
#[cqrsy::handler(command)]
pub fn execute(
    req: RemoveLiveView,
    store: &impl AppStateStore,
) -> Result<RemoveLiveViewResponse, RemoveLiveViewError> {
    let RemoveLiveView {
        data_root,
        source_kind,
        source_value,
    } = req;
    let removed = store.remove_live_view(&data_root, &source_kind, &source_value)?;
    Ok(RemoveLiveViewResponse { removed })
}

#[cfg(test)]
mod tests {
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
        let response = execute(
            RemoveLiveView {
                data_root: "/data".into(),
                source_kind: "LocalRepo".into(),
                source_value: "/repos/gt".into(),
            },
            &store,
        )
        .expect("remove succeeds");

        assert!(response.removed);
        assert!(store.live_views.lock().unwrap().is_empty());
    }

    #[test]
    fn remove_reports_false_for_unknown_identity() {
        let store = InMemoryAppStateStore::default();
        let response = execute(
            RemoveLiveView {
                data_root: "/data".into(),
                source_kind: "LocalRepo".into(),
                source_value: "/repos/unknown".into(),
            },
            &store,
        )
        .expect("remove succeeds");

        assert!(!response.removed);
    }
}
