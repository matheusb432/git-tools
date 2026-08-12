use gtl_wire::viewer::{
    GetViewerHistoryCopy, ListViewerHistory, OpenViewerHistory, ViewerHistoryCopyPayload,
    ViewerHistoryPage, ViewerShell,
};

use crate::shared::bridge::{ClientApiError, TauriBridge};

const LIST_HISTORY_COMMAND: &str = "viewer_list_history";
const OPEN_HISTORY_COMMAND: &str = "viewer_open_history";
const GET_HISTORY_COPY_COMMAND: &str = "viewer_get_history_copy";

pub(crate) struct DiffHistoryApi;

impl DiffHistoryApi {
    pub(crate) async fn list_history(
        request: ListViewerHistory,
    ) -> Result<ViewerHistoryPage, ClientApiError> {
        TauriBridge::invoke_request(LIST_HISTORY_COMMAND, &request).await
    }

    pub(crate) async fn open_history(
        request: OpenViewerHistory,
    ) -> Result<ViewerShell, ClientApiError> {
        TauriBridge::invoke_request(OPEN_HISTORY_COMMAND, &request).await
    }

    pub(crate) async fn get_history_copy(
        request: GetViewerHistoryCopy,
    ) -> Result<ViewerHistoryCopyPayload, ClientApiError> {
        TauriBridge::invoke_request(GET_HISTORY_COPY_COMMAND, &request).await
    }
}
