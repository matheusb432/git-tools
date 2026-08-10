use gtl_contracts::viewer::{
    GetViewerHistoryCopy, ListViewerHistory, OpenViewerHistory, ViewerHistoryCopyPayload,
    ViewerHistoryCursor, ViewerHistoryPage, ViewerShell,
};

use crate::shared::bridge::{ClientApiError, TauriBridge};

const LIST_HISTORY_COMMAND: &str = "viewer_list_history";
const OPEN_HISTORY_COMMAND: &str = "viewer_open_history";
const GET_HISTORY_COPY_COMMAND: &str = "viewer_get_history_copy";

pub(crate) struct DiffHistoryApi;

impl DiffHistoryApi {
    pub(crate) async fn list_history(
        cursor: ViewerHistoryCursor,
    ) -> Result<ViewerHistoryPage, ClientApiError> {
        TauriBridge::invoke_request(LIST_HISTORY_COMMAND, &ListViewerHistory { cursor }).await
    }

    pub(crate) async fn open_history(render_id: i64) -> Result<ViewerShell, ClientApiError> {
        TauriBridge::invoke_request(OPEN_HISTORY_COMMAND, &OpenViewerHistory { render_id }).await
    }

    pub(crate) async fn get_history_copy(
        render_id: i64,
    ) -> Result<ViewerHistoryCopyPayload, ClientApiError> {
        TauriBridge::invoke_request(
            GET_HISTORY_COPY_COMMAND,
            &GetViewerHistoryCopy { render_id },
        )
        .await
    }
}
