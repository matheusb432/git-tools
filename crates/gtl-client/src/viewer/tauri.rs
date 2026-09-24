use std::{cell::RefCell, collections::VecDeque};

use gtl_wire::viewer::{
    EditSettingsRequest, FindViewerDiff, GetViewerHistoryCopy, ListViewerCommits,
    ListViewerHistory, MoveViewerTab, OpenViewerDiffFile, OpenViewerHistory, RenameViewerSnapshot,
    SearchViewerFiles, SelectViewerCommit, SetViewerModifiedFiles, SetViewerPreference,
    SetViewerTabPinned, StreamViewerRows, ViewerCommitPage, ViewerDiffSearchResult,
    ViewerFileSearchResult, ViewerHistoryCopyPayload, ViewerHistoryPage, ViewerRowStreamItem,
    ViewerShell, ViewerStateChanged, ViewerTabRequest, ViewerUserSettings,
    projects::{
        DiscoverProjectRepositories, GetViewerProjectStatus, ImportProjectRepositories,
        ListViewerProjects, OpenViewerProject, OpenViewerProjectOk, ProjectDiscovery,
        ProjectImportResult, UpdateViewerProject, ViewerProjectPage, ViewerProjectStatus,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use wasm_bindgen::{JsCast as _, JsValue, prelude::wasm_bindgen};

use super::{ViewerClientError, validate_viewer_protocol};

const CONNECT_COMMAND: &str = "viewer_connect";
const ROWS_START_COMMAND: &str = "viewer_stream_rows_start";
const ROWS_NEXT_BATCH_COMMAND: &str = "viewer_stream_rows_next_batch";
const ROWS_CANCEL_COMMAND: &str = "viewer_stream_rows_cancel";
const WATCH_START_COMMAND: &str = "viewer_watch_start";
const WATCH_NEXT_BATCH_COMMAND: &str = "viewer_watch_next_batch";
const WATCH_CANCEL_COMMAND: &str = "viewer_watch_cancel";

pub async fn pick_project_folder() -> Result<Option<String>, ViewerClientError> {
    invoke_without_arguments("desktop_pick_project_folder").await
}

thread_local! {
    static CACHED_CONNECTION: RefCell<Option<ViewerConnection>> = const { RefCell::new(None) };
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewerConnection {
    instance_id: String,
    protocol_version: u32,
}

#[derive(Serialize)]
struct RequestArguments<Request> {
    request: Request,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamArguments {
    stream_id: u32,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(
        catch,
        js_namespace = ["window", "__TAURI_INTERNALS__"],
        js_name = invoke
    )]
    async fn invoke_tauri(command: &str, arguments: JsValue) -> Result<JsValue, JsValue>;
}

macro_rules! viewer_unary_methods {
    ($($name:ident($request:ty) -> $response:ty => $command:literal;)+) => {
        $(
            pub async fn $name(
                &mut self,
                request: $request,
            ) -> Result<$response, ViewerClientError> {
                invoke_with_request($command, request).await
            }
        )+
    };
}

#[derive(Clone)]
pub struct ViewerClient {
    server_instance_id: String,
    protocol_version: u32,
}

impl ViewerClient {
    viewer_unary_methods! {
        get_push_availability(gtl_wire::viewer::ViewerViewIdentity) -> gtl_wire::viewer::push::ViewerPushAvailability => "viewer_get_push_availability";
        create_push(gtl_wire::viewer::push::CreateViewerPush) -> gtl_wire::viewer::push::ViewerPushRequest => "viewer_create_push";
        get_push(gtl_wire::viewer::push::ViewerPushRequest) -> gtl_wire::viewer::push::ViewerPushStatus => "viewer_get_push";
        start_push(gtl_wire::viewer::push::ViewerPushRequest) -> () => "viewer_start_push";
    }

    pub async fn connect() -> Result<Self, ViewerClientError> {
        let connection = load_connection().await?;
        validate_viewer_protocol(connection.protocol_version)?;
        if connection.instance_id.is_empty() {
            return Err(ViewerClientError::Disconnected);
        }
        Ok(Self {
            server_instance_id: connection.instance_id,
            protocol_version: connection.protocol_version,
        })
    }

    pub fn discard_connection() {
        CACHED_CONNECTION.with(|cached| {
            cached.borrow_mut().take();
        });
    }

    #[must_use]
    pub fn server_instance_id(&self) -> &str {
        &self.server_instance_id
    }

    #[must_use]
    pub const fn protocol_version(&self) -> u32 {
        self.protocol_version
    }

    pub async fn get_shell(&mut self) -> Result<ViewerShell, ViewerClientError> {
        invoke_without_arguments("viewer_get_shell").await
    }

    viewer_unary_methods! {
        discover_project_repositories(DiscoverProjectRepositories) -> ProjectDiscovery => "viewer_discover_project_repositories";
        import_project_repositories(ImportProjectRepositories) -> Vec<ProjectImportResult> => "viewer_import_project_repositories";
        list_projects(ListViewerProjects) -> ViewerProjectPage => "viewer_list_projects";
        get_project_status(GetViewerProjectStatus) -> ViewerProjectStatus => "viewer_get_project_status";
        update_project(UpdateViewerProject) -> () => "viewer_update_project";
        open_project(OpenViewerProject) -> OpenViewerProjectOk => "viewer_open_project";
        activate_tab(ViewerTabRequest) -> ViewerShell => "viewer_activate_tab";
        move_tab(MoveViewerTab) -> ViewerShell => "viewer_move_tab";
        close_tab(ViewerTabRequest) -> ViewerShell => "viewer_close_tab";
        refresh_tab(ViewerTabRequest) -> ViewerShell => "viewer_refresh_tab";
        select_commit(SelectViewerCommit) -> ViewerShell => "viewer_select_commit";
        rename_snapshot(RenameViewerSnapshot) -> () => "viewer_rename_snapshot";
        set_tab_pinned(SetViewerTabPinned) -> () => "viewer_set_tab_pinned";
        close_other_tabs(ViewerTabRequest) -> () => "viewer_close_other_tabs";
        set_modified_files(SetViewerModifiedFiles) -> () => "viewer_set_modified_files";
        clear_commit_selection(ViewerTabRequest) -> ViewerShell => "viewer_clear_commit_selection";
        set_preference(SetViewerPreference) -> ViewerShell => "viewer_set_preference";
        list_commits(ListViewerCommits) -> ViewerCommitPage => "viewer_list_commits";
        search_files(SearchViewerFiles) -> ViewerFileSearchResult => "viewer_search_files";
        find_diff(FindViewerDiff) -> ViewerDiffSearchResult => "viewer_find_diff";
        read_diff_text(gtl_wire::viewer::ReadViewerDiffText) -> Vec<gtl_wire::viewer::ViewerDiffTextLine> => "viewer_read_diff_text";
        list_history(ListViewerHistory) -> ViewerHistoryPage => "viewer_list_history";
        open_history(OpenViewerHistory) -> ViewerShell => "viewer_open_history";
        get_history_copy(GetViewerHistoryCopy) -> ViewerHistoryCopyPayload => "viewer_get_history_copy";
        reset_settings(gtl_wire::viewer::ResetSettings) -> gtl_wire::viewer::ResetSettingsOk => "viewer_reset_settings";
        edit_settings(EditSettingsRequest) -> () => "viewer_edit_settings";
        get_file_filters(gtl_wire::viewer::ViewerTabRequest) -> gtl_wire::viewer::file_filters::ViewerFileFilters => "viewer_get_file_filters";
        set_file_filters(gtl_wire::viewer::file_filters::SetViewerFileFilters) -> () => "viewer_set_file_filters";
        update_diff_exclusions(gtl_wire::viewer::file_filters::UpdateDiffExclusions) -> () => "viewer_update_diff_exclusions";
        open_diff_file(OpenViewerDiffFile) -> () => "viewer_open_diff_file";
    }

    pub async fn get_settings_recovery(
        &mut self,
    ) -> Result<gtl_wire::viewer::ViewerSettingsRecovery, ViewerClientError> {
        invoke_without_arguments("viewer_get_settings_recovery").await
    }

    pub async fn get_settings(&mut self) -> Result<ViewerUserSettings, ViewerClientError> {
        invoke_without_arguments("viewer_get_settings").await
    }

    pub async fn stream_rows(
        &mut self,
        request: StreamViewerRows,
    ) -> Result<ViewerRowStream, ViewerClientError> {
        start_stream(ROWS_START_COMMAND, request, |stream_id| ViewerRowStream {
            stream: IpcPullStream::new(stream_id, ROWS_NEXT_BATCH_COMMAND, ROWS_CANCEL_COMMAND),
        })
        .await
    }

    pub async fn watch(
        &mut self,
        request: gtl_wire::viewer::WatchViewer,
    ) -> Result<ViewerVersionStream, ViewerClientError> {
        start_stream(WATCH_START_COMMAND, request, |stream_id| {
            ViewerVersionStream {
                stream: IpcPullStream::new(
                    stream_id,
                    WATCH_NEXT_BATCH_COMMAND,
                    WATCH_CANCEL_COMMAND,
                ),
            }
        })
        .await
    }
}

async fn start_stream<Request: Serialize + 'static, Stream: 'static>(
    command: &'static str,
    request: Request,
    own: fn(u32) -> Stream,
) -> Result<Stream, ViewerClientError> {
    let (sender, receiver) = futures_channel::oneshot::channel();
    wasm_bindgen_futures::spawn_local(super::stream_start::complete(
        async move { invoke_with_request(command, request).await.map(own) },
        sender,
    ));
    receiver
        .await
        .map_err(|_| ViewerClientError::Disconnected)?
}

pub struct ViewerRowStream {
    stream: IpcPullStream<ViewerRowStreamItem>,
}

impl ViewerRowStream {
    pub async fn message(&mut self) -> Result<Option<ViewerRowStreamItem>, ViewerClientError> {
        self.stream.message_with(invoke_row_batch).await
    }
}

impl Drop for ViewerRowStream {
    fn drop(&mut self) {
        self.stream.cancel();
    }
}

pub struct ViewerVersionStream {
    stream: IpcPullStream<ViewerStateChanged>,
}

impl ViewerVersionStream {
    pub async fn message(&mut self) -> Result<Option<ViewerStateChanged>, ViewerClientError> {
        self.stream.message().await
    }
}

impl Drop for ViewerVersionStream {
    fn drop(&mut self) {
        self.stream.cancel();
    }
}

struct IpcPullStream<Item> {
    stream_id: u32,
    next_batch_command: &'static str,
    cancel_command: &'static str,
    buffered_items: VecDeque<Item>,
    finished: bool,
}

impl<Item> IpcPullStream<Item> {
    fn new(stream_id: u32, next_batch_command: &'static str, cancel_command: &'static str) -> Self {
        Self {
            stream_id,
            next_batch_command,
            cancel_command,
            buffered_items: VecDeque::new(),
            finished: false,
        }
    }

    async fn message(&mut self) -> Result<Option<Item>, ViewerClientError>
    where
        Item: DeserializeOwned,
    {
        self.message_with(invoke_with_stream_id::<Vec<Item>>).await
    }

    async fn message_with<F, Fut>(
        &mut self,
        read_batch: F,
    ) -> Result<Option<Item>, ViewerClientError>
    where
        F: FnOnce(&'static str, u32) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<Item>, ViewerClientError>>,
    {
        if let Some(item) = self.buffered_items.pop_front() {
            return Ok(Some(item));
        }
        if self.finished {
            return Ok(None);
        }
        let batch = read_batch(self.next_batch_command, self.stream_id).await?;
        if batch.is_empty() {
            self.finished = true;
            return Ok(None);
        }
        self.buffered_items = batch.into();
        Ok(self.buffered_items.pop_front())
    }

    fn cancel(&mut self) {
        if !self.finished {
            self.finished = true;
            cancel_stream(self.cancel_command, self.stream_id);
        }
    }
}

async fn load_connection() -> Result<ViewerConnection, ViewerClientError> {
    if let Some(connection) = CACHED_CONNECTION.with(|cached| cached.borrow().as_ref().cloned()) {
        return Ok(connection);
    }
    let connection: ViewerConnection = invoke_without_arguments(CONNECT_COMMAND).await?;
    CACHED_CONNECTION.with(|cached| {
        cached.replace(Some(connection.clone()));
    });
    Ok(connection)
}

pub(crate) async fn invoke_without_arguments<Response>(
    command: &str,
) -> Result<Response, ViewerClientError>
where
    Response: DeserializeOwned,
{
    invoke(command, JsValue::UNDEFINED).await
}

pub(crate) async fn invoke_with_request<Request, Response>(
    command: &str,
    request: Request,
) -> Result<Response, ViewerClientError>
where
    Request: Serialize,
    Response: DeserializeOwned,
{
    let arguments = serde_wasm_bindgen::to_value(&RequestArguments { request })
        .map_err(|_| ViewerClientError::InvalidMessage)?;
    invoke(command, arguments).await
}

async fn invoke_with_stream_id<Response>(
    command: &str,
    stream_id: u32,
) -> Result<Response, ViewerClientError>
where
    Response: DeserializeOwned,
{
    let arguments = serde_wasm_bindgen::to_value(&StreamArguments { stream_id })
        .map_err(|_| ViewerClientError::InvalidMessage)?;
    invoke(command, arguments).await
}

async fn invoke<Response>(command: &str, arguments: JsValue) -> Result<Response, ViewerClientError>
where
    Response: DeserializeOwned,
{
    let value = invoke_value(command, arguments).await?;
    serde_wasm_bindgen::from_value(value).map_err(|_| ViewerClientError::InvalidMessage)
}

async fn invoke_row_batch(
    command: &'static str,
    stream_id: u32,
) -> Result<Vec<ViewerRowStreamItem>, ViewerClientError> {
    let arguments = serde_wasm_bindgen::to_value(&StreamArguments { stream_id })
        .map_err(|_| ViewerClientError::InvalidMessage)?;
    let value = invoke_value(command, arguments).await?;
    let bytes = row_bytes(value)?;
    gtl_wire::proto::row_ipc::decode_batch(&bytes).map_err(|_| ViewerClientError::InvalidMessage)
}

fn row_bytes(value: JsValue) -> Result<Vec<u8>, ViewerClientError> {
    let maximum = gtl_wire::proto::row_ipc::ROW_IPC_BATCH_BYTES_MAX;
    if let Some(buffer) = value.dyn_ref::<js_sys::ArrayBuffer>() {
        if buffer.byte_length() as usize > maximum {
            return Err(ViewerClientError::InvalidMessage);
        }
        return Ok(js_sys::Uint8Array::new(buffer).to_vec());
    }
    // Tauri uses a numeric byte array on platforms without custom-protocol responses.
    let array = value
        .dyn_into::<js_sys::Array>()
        .map_err(|_| ViewerClientError::InvalidMessage)?;
    if array.length() as usize > maximum {
        return Err(ViewerClientError::InvalidMessage);
    }
    if !array.iter().all(|byte| {
        byte.as_f64()
            .is_some_and(|byte| byte.fract() == 0.0 && (0.0..=255.0).contains(&byte))
    }) {
        return Err(ViewerClientError::InvalidMessage);
    }
    Ok(js_sys::Uint8Array::new(&array).to_vec())
}

async fn invoke_value(command: &str, arguments: JsValue) -> Result<JsValue, ViewerClientError> {
    let value = invoke_tauri(command, arguments).await.map_err(|value| {
        let error =
            serde_wasm_bindgen::from_value(value).unwrap_or(ViewerClientError::Disconnected);
        if error == ViewerClientError::Disconnected {
            ViewerClient::discard_connection();
        }
        error
    })?;
    Ok(value)
}

fn cancel_stream(command: &'static str, stream_id: u32) {
    wasm_bindgen_futures::spawn_local(async move {
        let _ = invoke_with_stream_id::<()>(command, stream_id).await;
    });
}
