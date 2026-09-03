use std::{cell::RefCell, collections::VecDeque};

use gtl_wire::viewer::{
    EditSettingsRequest, FindViewerDiff, GetViewerHistoryCopy, ListViewerCommits,
    ListViewerHistory, OpenViewerDiffFile, OpenViewerHistory, SearchViewerFiles,
    SelectViewerCommit, SetViewerPreference, StreamViewerRows, ViewerCommitPage,
    ViewerDiffSearchResult, ViewerFileSearchResult, ViewerHistoryCopyPayload, ViewerHistoryPage,
    ViewerRowStreamItem, ViewerShell, ViewerStateChanged, ViewerTabRequest, ViewerUserSettings,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

use super::{ViewerClientError, validate_viewer_protocol};

const CONNECT_COMMAND: &str = "viewer_connect";
const ROWS_START_COMMAND: &str = "viewer_stream_rows_start";
const ROWS_NEXT_BATCH_COMMAND: &str = "viewer_stream_rows_next_batch";
const ROWS_CANCEL_COMMAND: &str = "viewer_stream_rows_cancel";
const WATCH_START_COMMAND: &str = "viewer_watch_start";
const WATCH_NEXT_BATCH_COMMAND: &str = "viewer_watch_next_batch";
const WATCH_CANCEL_COMMAND: &str = "viewer_watch_cancel";

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
    pub async fn connect() -> Result<Self, ViewerClientError> {
        let connection = load_connection().await?;
        validate_viewer_protocol(connection.protocol_version)?;
        if connection.instance_id.is_empty() {
            return Err(ViewerClientError::Unavailable);
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
        activate_tab(ViewerTabRequest) -> ViewerShell => "viewer_activate_tab";
        close_tab(ViewerTabRequest) -> ViewerShell => "viewer_close_tab";
        refresh_tab(ViewerTabRequest) -> ViewerShell => "viewer_refresh_tab";
        delete_live_tab(ViewerTabRequest) -> ViewerShell => "viewer_delete_live_tab";
        select_commit(SelectViewerCommit) -> ViewerShell => "viewer_select_commit";
        clear_commit_selection(ViewerTabRequest) -> ViewerShell => "viewer_clear_commit_selection";
        set_preference(SetViewerPreference) -> ViewerShell => "viewer_set_preference";
        list_commits(ListViewerCommits) -> ViewerCommitPage => "viewer_list_commits";
        search_files(SearchViewerFiles) -> ViewerFileSearchResult => "viewer_search_files";
        find_diff(FindViewerDiff) -> ViewerDiffSearchResult => "viewer_find_diff";
        list_history(ListViewerHistory) -> ViewerHistoryPage => "viewer_list_history";
        open_history(OpenViewerHistory) -> ViewerShell => "viewer_open_history";
        get_history_copy(GetViewerHistoryCopy) -> ViewerHistoryCopyPayload => "viewer_get_history_copy";
        edit_settings(EditSettingsRequest) -> () => "viewer_edit_settings";
        open_diff_file(OpenViewerDiffFile) -> () => "viewer_open_diff_file";
    }

    pub async fn get_settings(&mut self) -> Result<ViewerUserSettings, ViewerClientError> {
        invoke_without_arguments("viewer_get_settings").await
    }

    pub async fn stream_rows(
        &mut self,
        request: StreamViewerRows,
    ) -> Result<ViewerRowStream, ViewerClientError> {
        let stream_id = invoke_with_request(ROWS_START_COMMAND, request).await?;
        Ok(ViewerRowStream {
            stream: IpcPullStream::new(stream_id, ROWS_NEXT_BATCH_COMMAND, ROWS_CANCEL_COMMAND),
        })
    }

    pub async fn watch(&mut self) -> Result<ViewerVersionStream, ViewerClientError> {
        let stream_id = invoke_without_arguments(WATCH_START_COMMAND).await?;
        Ok(ViewerVersionStream {
            stream: IpcPullStream::new(stream_id, WATCH_NEXT_BATCH_COMMAND, WATCH_CANCEL_COMMAND),
        })
    }
}

pub struct ViewerRowStream {
    stream: IpcPullStream<ViewerRowStreamItem>,
}

impl ViewerRowStream {
    pub async fn message(&mut self) -> Result<Option<ViewerRowStreamItem>, ViewerClientError> {
        self.stream.message().await
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

impl<Item> IpcPullStream<Item>
where
    Item: DeserializeOwned,
{
    fn new(stream_id: u32, next_batch_command: &'static str, cancel_command: &'static str) -> Self {
        Self {
            stream_id,
            next_batch_command,
            cancel_command,
            buffered_items: VecDeque::new(),
            finished: false,
        }
    }

    async fn message(&mut self) -> Result<Option<Item>, ViewerClientError> {
        if let Some(item) = self.buffered_items.pop_front() {
            return Ok(Some(item));
        }
        if self.finished {
            return Ok(None);
        }
        let batch: Vec<Item> =
            invoke_with_stream_id(self.next_batch_command, self.stream_id).await?;
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

async fn invoke_without_arguments<Response>(command: &str) -> Result<Response, ViewerClientError>
where
    Response: DeserializeOwned,
{
    invoke(command, JsValue::UNDEFINED).await
}

async fn invoke_with_request<Request, Response>(
    command: &str,
    request: Request,
) -> Result<Response, ViewerClientError>
where
    Request: Serialize,
    Response: DeserializeOwned,
{
    let arguments = serde_wasm_bindgen::to_value(&RequestArguments { request })
        .map_err(|_| ViewerClientError::InvalidRequest)?;
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
        .map_err(|_| ViewerClientError::Internal)?;
    invoke(command, arguments).await
}

async fn invoke<Response>(command: &str, arguments: JsValue) -> Result<Response, ViewerClientError>
where
    Response: DeserializeOwned,
{
    let value = invoke_tauri(command, arguments).await.map_err(|value| {
        let error = serde_wasm_bindgen::from_value(value).unwrap_or(ViewerClientError::Unavailable);
        if error == ViewerClientError::Unavailable {
            ViewerClient::discard_connection();
        }
        error
    })?;
    serde_wasm_bindgen::from_value(value).map_err(|_| ViewerClientError::Internal)
}

fn cancel_stream(command: &'static str, stream_id: u32) {
    wasm_bindgen_futures::spawn_local(async move {
        let _ = invoke_with_stream_id::<()>(command, stream_id).await;
    });
}
