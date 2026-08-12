use gtl_wire::viewer::{
    LoadViewerDiffLines, OpenViewerDiffFile, SelectViewerCommit, SetViewerPreference,
    VIEWER_STATE_CHANGED_EVENT, ViewerDiffLines, ViewerShell, ViewerStateChanged, ViewerTabRequest,
};

use crate::shared::bridge::{ClientApiError, TauriBridge};

const GET_SHELL_COMMAND: &str = "viewer_get_shell";
const LOAD_DIFF_LINES_COMMAND: &str = "viewer_load_diff_lines";
const ACTIVATE_TAB_COMMAND: &str = "viewer_activate_tab";
const CLOSE_TAB_COMMAND: &str = "viewer_close_tab";
const REFRESH_TAB_COMMAND: &str = "viewer_refresh_tab";
const DELETE_LIVE_TAB_COMMAND: &str = "viewer_delete_live_tab";
const SELECT_COMMIT_COMMAND: &str = "viewer_select_commit";
const CLEAR_COMMIT_SELECTION_COMMAND: &str = "viewer_clear_commit_selection";
const SET_PREFERENCE_COMMAND: &str = "viewer_set_preference";
const OPEN_DIFF_FILE_COMMAND: &str = "viewer_open_diff_file";

pub(crate) struct DiffViewerApi;

impl DiffViewerApi {
    pub(crate) async fn get_shell() -> Result<ViewerShell, ClientApiError> {
        TauriBridge::invoke(GET_SHELL_COMMAND).await
    }

    pub(crate) async fn load_diff_lines(
        request: LoadViewerDiffLines,
    ) -> Result<ViewerDiffLines, ClientApiError> {
        TauriBridge::invoke_request(LOAD_DIFF_LINES_COMMAND, &request).await
    }

    pub(crate) async fn activate_tab(
        request: ViewerTabRequest,
    ) -> Result<ViewerShell, ClientApiError> {
        TauriBridge::invoke_request(ACTIVATE_TAB_COMMAND, &request).await
    }

    pub(crate) async fn close_tab(
        request: ViewerTabRequest,
    ) -> Result<ViewerShell, ClientApiError> {
        TauriBridge::invoke_request(CLOSE_TAB_COMMAND, &request).await
    }

    pub(crate) async fn refresh_tab(
        request: ViewerTabRequest,
    ) -> Result<ViewerShell, ClientApiError> {
        TauriBridge::invoke_request(REFRESH_TAB_COMMAND, &request).await
    }

    pub(crate) async fn delete_live_tab(
        request: ViewerTabRequest,
    ) -> Result<ViewerShell, ClientApiError> {
        TauriBridge::invoke_request(DELETE_LIVE_TAB_COMMAND, &request).await
    }

    pub(crate) async fn select_commit(
        request: SelectViewerCommit,
    ) -> Result<ViewerShell, ClientApiError> {
        TauriBridge::invoke_request(SELECT_COMMIT_COMMAND, &request).await
    }

    pub(crate) async fn clear_commit_selection(
        request: ViewerTabRequest,
    ) -> Result<ViewerShell, ClientApiError> {
        TauriBridge::invoke_request(CLEAR_COMMIT_SELECTION_COMMAND, &request).await
    }

    pub(crate) async fn set_preference(
        preference: SetViewerPreference,
    ) -> Result<ViewerShell, ClientApiError> {
        TauriBridge::invoke_request(SET_PREFERENCE_COMMAND, &preference).await
    }

    pub(crate) async fn open_diff_file(request: OpenViewerDiffFile) -> Result<(), ClientApiError> {
        TauriBridge::invoke_request(OPEN_DIFF_FILE_COMMAND, &request).await
    }

    pub(crate) async fn listen_for_state_changes<Ready, Handler>(
        on_ready: Ready,
        on_event: Handler,
    ) -> Result<(), ClientApiError>
    where
        Ready: Fn() + 'static,
        Handler: Fn(ViewerStateChanged) + 'static,
    {
        TauriBridge::listen_to_event(VIEWER_STATE_CHANGED_EVENT, on_ready, on_event).await
    }
}
