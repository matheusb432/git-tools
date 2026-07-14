//! The `live_views/list` vertical slice: every saved live view, creation order.

use std::path::PathBuf;

use crate::ports::{AppStateStore, LiveViewRecord};

/// List every saved live view under `data_root`.
#[derive(Debug, Clone, PartialEq)]
pub struct ListLiveViews {
    pub data_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListLiveViewsResponse {
    pub views: Vec<LiveViewRecord>,
}

#[derive(Debug, thiserror::Error)]
pub enum ListLiveViewsError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Lists saved live views through the app-state port.
#[cqrsy::handler(query)]
pub fn execute(
    req: ListLiveViews,
    store: &impl AppStateStore,
) -> Result<ListLiveViewsResponse, ListLiveViewsError> {
    let ListLiveViews { data_root } = req;
    let views = store.list_live_views(&data_root)?;
    Ok(ListLiveViewsResponse { views })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ports::LiveViewRecord, testing::InMemoryAppStateStore};

    fn record(value: &str) -> LiveViewRecord {
        LiveViewRecord {
            source_kind: "LocalRepo".into(),
            source_value: value.into(),
            display_name: value.into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            last_opened_at: None,
        }
    }

    #[test]
    fn lists_saved_views_in_creation_order() {
        let store = InMemoryAppStateStore::default();
        store.live_views.lock().unwrap().push(record("/repos/a"));
        store.live_views.lock().unwrap().push(record("/repos/b"));
        let response = execute(
            ListLiveViews {
                data_root: "/data".into(),
            },
            &store,
        )
        .expect("list succeeds");

        assert_eq!(
            response
                .views
                .iter()
                .map(|v| v.source_value.as_str())
                .collect::<Vec<_>>(),
            vec!["/repos/a", "/repos/b"]
        );
    }
}
