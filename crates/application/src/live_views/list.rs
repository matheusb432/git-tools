//! The `live_views/list` vertical slice: every saved live view, creation order.

use std::path::PathBuf;

use cqrsy::Handler;

use crate::ports::{AppStateStore, LiveViewRecord};

/// List every saved live view under `data_root`.
#[derive(Debug, Clone, PartialEq, cqrsy::Query)]
#[query(out = ListLiveViewsResponse, err = ListLiveViewsError)]
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

/// Handles [`ListLiveViews`] by reading through the app-state port.
#[derive(Clone)]
pub struct ListLiveViewsHandler<A: AppStateStore> {
    pub store: A,
}

impl<A: AppStateStore> Handler<ListLiveViews> for ListLiveViewsHandler<A> {
    async fn handle(
        &self,
        req: ListLiveViews,
    ) -> Result<ListLiveViewsResponse, ListLiveViewsError> {
        let views = self.store.list_live_views(&req.data_root)?;
        Ok(ListLiveViewsResponse { views })
    }
}

#[cfg(test)]
mod tests {
    use cqrsy::send_now;

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
        let handler = ListLiveViewsHandler { store };

        let response = send_now(
            &(),
            &handler,
            ListLiveViews {
                data_root: "/data".into(),
            },
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
