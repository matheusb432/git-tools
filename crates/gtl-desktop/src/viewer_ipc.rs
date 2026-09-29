use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use gtl_client::{ViewerClient, ViewerClientError, ViewerRowStream, ViewerVersionStream};
use gtl_wire::viewer::{
    EditSettingsRequest, FindViewerDiff, GetViewerHistoryCopy, ListViewerCommits,
    ListViewerHistory, MoveViewerTab, OpenViewerDiffFile, OpenViewerHistory, RenameViewerSnapshot,
    SearchViewerFiles, SelectViewerCommit, SetViewerModifiedFiles, SetViewerPreference,
    SetViewerTabLive, SetViewerTabPinned, StreamViewerRows, ViewerCommitPage,
    ViewerDiffSearchResult, ViewerFileSearchResult, ViewerHistoryCopyPayload, ViewerHistoryPage,
    ViewerShell, ViewerStateChanged, ViewerTabRequest, ViewerUserSettings,
    projects::{
        DiscoverProjectRepositories, GetViewerProjectStatus, ImportProjectRepositories,
        ListViewerProjects, OpenViewerProject, OpenViewerProjectOk, ProjectDiscovery,
        ProjectImportResult, UpdateViewerProject, ViewerProjectPage, ViewerProjectStatus,
    },
};
use serde::Serialize;
use tauri::State;
use tokio::sync::{Mutex, mpsc, oneshot};

const MAX_ACTIVE_STREAMS: usize = 10;
const STREAM_BATCH_ITEMS_MAX: usize = gtl_wire::proto::row_ipc::ROW_IPC_BATCH_ITEMS_MAX;
const STREAM_BATCH_LINGER: Duration = Duration::from_millis(1);
const STREAM_REQUEST_BUFFER: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ViewerConnectionPayload {
    instance_id: String,
    protocol_version: u32,
}

pub(crate) struct ViewerIpcState {
    client: Mutex<Option<ViewerClient>>,
    row_streams: PullStreamRegistry<Vec<u8>>,
    version_streams: PullStreamRegistry<ViewerStateChanged>,
}

impl Default for ViewerIpcState {
    fn default() -> Self {
        Self {
            client: Mutex::new(None),
            row_streams: PullStreamRegistry::new(MAX_ACTIVE_STREAMS),
            version_streams: PullStreamRegistry::new(MAX_ACTIVE_STREAMS),
        }
    }
}

impl ViewerIpcState {
    async fn connect(&self) -> Result<ViewerConnectionPayload, ViewerClientError> {
        let client = ViewerClient::connect_local().await?;
        let payload = ViewerConnectionPayload {
            instance_id: client.server_instance_id().to_owned(),
            protocol_version: client.protocol_version(),
        };
        let replaced_instance = {
            let mut current = self.client.lock().await;
            let replaced_instance = current
                .as_ref()
                .is_some_and(|current| current.server_instance_id() != payload.instance_id);
            current.replace(client);
            replaced_instance
        };
        if replaced_instance {
            self.cancel_all_streams().await;
        }
        Ok(payload)
    }

    async fn client(&self) -> Result<ViewerClient, ViewerClientError> {
        self.client
            .lock()
            .await
            .clone()
            .ok_or(ViewerClientError::Disconnected)
    }

    async fn observe_result<ResultType>(
        &self,
        instance_id: &str,
        result: &Result<ResultType, ViewerClientError>,
    ) {
        if !matches!(result, Err(ViewerClientError::Disconnected)) {
            return;
        }
        let removed = {
            let mut current = self.client.lock().await;
            take_matching_client(&mut current, instance_id)
        };
        if removed {
            self.cancel_all_streams().await;
        }
    }

    async fn cancel_all_streams(&self) {
        self.row_streams.cancel_all().await;
        self.version_streams.cancel_all().await;
    }
}

fn take_matching_client(current: &mut Option<ViewerClient>, instance_id: &str) -> bool {
    if current
        .as_ref()
        .is_some_and(|current| current.server_instance_id() == instance_id)
    {
        current.take();
        true
    } else {
        false
    }
}

macro_rules! viewer_query_command {
    ($name:ident, $response:ty, $method:ident) => {
        #[tauri::command]
        pub(crate) async fn $name(
            state: State<'_, ViewerIpcState>,
        ) -> Result<$response, ViewerClientError> {
            let mut client = state.client().await?;
            let instance_id = client.server_instance_id().to_owned();
            let result = client.$method().await;
            state.observe_result(&instance_id, &result).await;
            result
        }
    };
}

macro_rules! viewer_request_command {
    ($name:ident, $request:ty, $response:ty, $method:ident) => {
        #[tauri::command]
        pub(crate) async fn $name(
            state: State<'_, ViewerIpcState>,
            request: $request,
        ) -> Result<$response, ViewerClientError> {
            let mut client = state.client().await?;
            let instance_id = client.server_instance_id().to_owned();
            let result = client.$method(request).await;
            state.observe_result(&instance_id, &result).await;
            result
        }
    };
}

// Tauri's command macro expands to an unreachable fallback arm.
#[allow(clippy::unreachable)]
mod connect_command {
    use super::{State, ViewerClientError, ViewerConnectionPayload, ViewerIpcState};

    #[tauri::command]
    pub(crate) async fn viewer_connect(
        state: State<'_, ViewerIpcState>,
    ) -> Result<ViewerConnectionPayload, ViewerClientError> {
        state.connect().await
    }
}

pub(crate) use connect_command::viewer_connect;

viewer_query_command!(viewer_get_shell, ViewerShell, get_shell);
viewer_request_command!(
    viewer_activate_tab,
    ViewerTabRequest,
    ViewerShell,
    activate_tab
);
viewer_request_command!(viewer_move_tab, MoveViewerTab, ViewerShell, move_tab);
viewer_request_command!(viewer_close_tab, ViewerTabRequest, ViewerShell, close_tab);
viewer_request_command!(
    viewer_refresh_tab,
    ViewerTabRequest,
    ViewerShell,
    refresh_tab
);
viewer_request_command!(viewer_update_tab, ViewerTabRequest, ViewerShell, update_tab);
viewer_request_command!(
    viewer_set_tab_live,
    SetViewerTabLive,
    ViewerShell,
    set_tab_live
);
viewer_request_command!(
    viewer_select_commit,
    SelectViewerCommit,
    ViewerShell,
    select_commit
);
viewer_request_command!(
    viewer_clear_commit_selection,
    ViewerTabRequest,
    ViewerShell,
    clear_commit_selection
);
viewer_request_command!(
    viewer_set_preference,
    SetViewerPreference,
    ViewerShell,
    set_preference
);
viewer_request_command!(
    viewer_list_commits,
    ListViewerCommits,
    ViewerCommitPage,
    list_commits
);
viewer_request_command!(
    viewer_search_files,
    SearchViewerFiles,
    ViewerFileSearchResult,
    search_files
);
viewer_request_command!(
    viewer_find_diff,
    FindViewerDiff,
    ViewerDiffSearchResult,
    find_diff
);
viewer_request_command!(
    viewer_read_diff_text,
    gtl_wire::viewer::ReadViewerDiffText,
    Vec<gtl_wire::viewer::ViewerDiffTextLine>,
    read_diff_text
);
viewer_request_command!(
    viewer_list_history,
    ListViewerHistory,
    ViewerHistoryPage,
    list_history
);
viewer_request_command!(
    viewer_open_history,
    OpenViewerHistory,
    ViewerShell,
    open_history
);
viewer_request_command!(
    viewer_get_history_copy,
    GetViewerHistoryCopy,
    ViewerHistoryCopyPayload,
    get_history_copy
);
viewer_query_command!(
    viewer_get_settings_recovery,
    gtl_wire::viewer::ViewerSettingsRecovery,
    get_settings_recovery
);
viewer_request_command!(
    viewer_reset_settings,
    gtl_wire::viewer::ResetSettings,
    gtl_wire::viewer::ResetSettingsOk,
    reset_settings
);
viewer_query_command!(viewer_get_settings, ViewerUserSettings, get_settings);
viewer_request_command!(viewer_edit_settings, EditSettingsRequest, (), edit_settings);
viewer_request_command!(
    viewer_open_diff_file,
    OpenViewerDiffFile,
    (),
    open_diff_file
);

// Tauri's command macro expands to an unreachable fallback arm.
#[allow(clippy::unreachable)]
mod stream_commands {
    use super::{State, StreamViewerRows, ViewerClientError, ViewerIpcState, ViewerStateChanged};

    #[tauri::command]
    pub(crate) async fn viewer_stream_rows_start(
        state: State<'_, ViewerIpcState>,
        request: StreamViewerRows,
    ) -> Result<u32, ViewerClientError> {
        let mut client = state.client().await?;
        let instance_id = client.server_instance_id().to_owned();
        let result = state
            .row_streams
            .start_with(client.stream_rows(request))
            .await;
        state.observe_result(&instance_id, &result).await;
        result
    }

    #[tauri::command]
    pub(crate) async fn viewer_stream_rows_next_batch(
        state: State<'_, ViewerIpcState>,
        stream_id: u32,
    ) -> Result<tauri::ipc::Response, ViewerClientError> {
        let frames = state.row_streams.next_batch(stream_id).await?;
        let bytes = gtl_wire::proto::row_ipc::encode_batch(&frames)
            .map_err(|_| ViewerClientError::InvalidMessage)?;
        Ok(tauri::ipc::Response::new(bytes))
    }

    #[tauri::command]
    pub(crate) async fn viewer_stream_rows_cancel(
        state: State<'_, ViewerIpcState>,
        stream_id: u32,
    ) -> Result<(), ViewerClientError> {
        state.row_streams.cancel(stream_id).await
    }

    #[tauri::command]
    pub(crate) async fn viewer_watch_start(
        state: State<'_, ViewerIpcState>,
        request: gtl_wire::viewer::WatchViewer,
    ) -> Result<u32, ViewerClientError> {
        let mut client = state.client().await?;
        let instance_id = client.server_instance_id().to_owned();
        let result = state
            .version_streams
            .start_with(client.watch(request))
            .await;
        state.observe_result(&instance_id, &result).await;
        result
    }

    #[tauri::command]
    pub(crate) async fn viewer_watch_next_batch(
        state: State<'_, ViewerIpcState>,
        stream_id: u32,
    ) -> Result<Vec<ViewerStateChanged>, ViewerClientError> {
        state.version_streams.next_batch(stream_id).await
    }

    #[tauri::command]
    pub(crate) async fn viewer_watch_cancel(
        state: State<'_, ViewerIpcState>,
        stream_id: u32,
    ) -> Result<(), ViewerClientError> {
        state.version_streams.cancel(stream_id).await
    }
}

pub(crate) use stream_commands::{
    viewer_stream_rows_cancel, viewer_stream_rows_next_batch, viewer_stream_rows_start,
    viewer_watch_cancel, viewer_watch_next_batch, viewer_watch_start,
};

trait PullStream: Send + 'static {
    type Item: Send + 'static;

    fn message(
        &mut self,
    ) -> impl Future<Output = Result<Option<Self::Item>, ViewerClientError>> + Send;
}

impl PullStream for ViewerRowStream {
    type Item = Vec<u8>;

    fn message(
        &mut self,
    ) -> impl Future<Output = Result<Option<Self::Item>, ViewerClientError>> + Send {
        ViewerRowStream::message_bytes(self)
    }
}

impl PullStream for ViewerVersionStream {
    type Item = ViewerStateChanged;

    fn message(
        &mut self,
    ) -> impl Future<Output = Result<Option<Self::Item>, ViewerClientError>> + Send {
        ViewerVersionStream::message(self)
    }
}

type StreamItemResult<Item> = Result<Option<Item>, ViewerClientError>;
type NextBatchResult<Item> = Result<Vec<Item>, ViewerClientError>;
type NextRequest<Item> = oneshot::Sender<NextBatchResult<Item>>;

struct StreamEntry<Item> {
    requests: mpsc::Sender<NextRequest<Item>>,
    abort: tokio::task::AbortHandle,
}

struct RegistryState<Item> {
    next_id: u32,
    entries: HashMap<u32, StreamEntry<Item>>,
}

struct PullStreamRegistry<Item> {
    limit: usize,
    start_gate: Mutex<()>,
    state: Arc<Mutex<RegistryState<Item>>>,
}

impl<Item> PullStreamRegistry<Item>
where
    Item: Send + 'static,
{
    fn new(limit: usize) -> Self {
        Self {
            limit,
            start_gate: Mutex::new(()),
            state: Arc::new(Mutex::new(RegistryState {
                next_id: 1,
                entries: HashMap::new(),
            })),
        }
    }

    async fn start_with<Stream, Start>(&self, start: Start) -> Result<u32, ViewerClientError>
    where
        Stream: PullStream<Item = Item>,
        Start: Future<Output = Result<Stream, ViewerClientError>>,
    {
        let _start_guard = self.start_gate.lock().await;
        let state = self.state.lock().await;
        if state.entries.len() >= self.limit {
            return Err(ViewerClientError::busy());
        }
        drop(state);
        let stream = start.await?;
        let mut state = self.state.lock().await;
        let stream_id = next_stream_id(&mut state);
        let (requests, receiver) = mpsc::channel(STREAM_REQUEST_BUFFER);
        let registry = Arc::clone(&self.state);
        let task = tokio::spawn(run_stream(stream_id, stream, receiver, registry));
        state.entries.insert(
            stream_id,
            StreamEntry {
                requests,
                abort: task.abort_handle(),
            },
        );
        Ok(stream_id)
    }

    #[cfg(test)]
    async fn start<Stream>(&self, stream: Stream) -> Result<u32, ViewerClientError>
    where
        Stream: PullStream<Item = Item>,
    {
        self.start_with(std::future::ready(Ok(stream))).await
    }

    async fn next_batch(&self, stream_id: u32) -> NextBatchResult<Item> {
        let requests = self
            .state
            .lock()
            .await
            .entries
            .get(&stream_id)
            .map(|entry| entry.requests.clone())
            .ok_or(ViewerClientError::StreamClosed)?;
        let (reply, response) = oneshot::channel();
        if requests.send(reply).await.is_err() {
            self.remove(stream_id).await;
            return Err(ViewerClientError::StreamClosed);
        }
        let Ok(result) = response.await else {
            self.remove(stream_id).await;
            return Err(ViewerClientError::StreamClosed);
        };
        if !matches!(&result, Ok(items) if !items.is_empty()) {
            self.remove(stream_id).await;
        }
        result
    }

    async fn cancel(&self, stream_id: u32) -> Result<(), ViewerClientError> {
        self.remove(stream_id)
            .await
            .ok_or(ViewerClientError::StreamClosed)
    }

    async fn cancel_all(&self) {
        let entries = {
            let mut state = self.state.lock().await;
            state
                .entries
                .drain()
                .map(|(_, entry)| entry)
                .collect::<Vec<_>>()
        };
        for entry in entries {
            entry.abort.abort();
        }
    }

    async fn remove(&self, stream_id: u32) -> Option<()> {
        let entry = self.state.lock().await.entries.remove(&stream_id)?;
        entry.abort.abort();
        Some(())
    }
}

fn next_stream_id<Item>(state: &mut RegistryState<Item>) -> u32 {
    loop {
        let candidate = state.next_id;
        state.next_id = state.next_id.checked_add(1).unwrap_or(1);
        if !state.entries.contains_key(&candidate) {
            return candidate;
        }
    }
}

async fn run_stream<Stream>(
    stream_id: u32,
    mut stream: Stream,
    mut requests: mpsc::Receiver<NextRequest<Stream::Item>>,
    registry: Arc<Mutex<RegistryState<Stream::Item>>>,
) where
    Stream: PullStream,
{
    let mut pending_terminal = None;
    while let Some(reply) = requests.recv().await {
        let mut items = Vec::with_capacity(STREAM_BATCH_ITEMS_MAX);
        let first = match pending_terminal.take() {
            Some(terminal) => terminal,
            None => stream.message().await,
        };
        let (result, finished) =
            collect_ready_batch(&mut stream, first, &mut items, &mut pending_terminal).await;
        let abandoned = reply.send(result).is_err();
        if finished || abandoned {
            break;
        }
    }
    registry.lock().await.entries.remove(&stream_id);
}

async fn collect_ready_batch<Stream>(
    stream: &mut Stream,
    mut next: StreamItemResult<Stream::Item>,
    items: &mut Vec<Stream::Item>,
    pending_terminal: &mut Option<StreamItemResult<Stream::Item>>,
) -> (NextBatchResult<Stream::Item>, bool)
where
    Stream: PullStream,
{
    loop {
        match next {
            Ok(Some(item)) => items.push(item),
            Ok(None) if items.is_empty() => return (Ok(Vec::new()), true),
            Ok(None) => {
                pending_terminal.replace(Ok(None));
                return (Ok(std::mem::take(items)), false);
            }
            Err(error) if items.is_empty() => return (Err(error), true),
            Err(error) => {
                pending_terminal.replace(Err(error));
                return (Ok(std::mem::take(items)), false);
            }
        }

        if items.len() == STREAM_BATCH_ITEMS_MAX {
            return (Ok(std::mem::take(items)), false);
        }
        let Some(ready) = tokio::time::timeout(STREAM_BATCH_LINGER, stream.message())
            .await
            .ok()
        else {
            return (Ok(std::mem::take(items)), false);
        };
        next = ready;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        future::pending,
        sync::atomic::{AtomicBool, Ordering},
    };

    use super::*;

    struct FiniteStream {
        items: VecDeque<StreamItemResult<u32>>,
    }

    impl PullStream for FiniteStream {
        type Item = u32;

        fn message(&mut self) -> impl Future<Output = StreamItemResult<Self::Item>> + Send {
            std::future::ready(self.items.pop_front().unwrap_or(Ok(None)))
        }
    }

    struct PendingStream;

    impl PullStream for PendingStream {
        type Item = u32;

        async fn message(&mut self) -> StreamItemResult<Self::Item> {
            pending().await
        }
    }

    struct OneThenPendingStream {
        item: Option<u32>,
    }

    impl PullStream for OneThenPendingStream {
        type Item = u32;

        async fn message(&mut self) -> StreamItemResult<Self::Item> {
            match self.item.take() {
                Some(item) => Ok(Some(item)),
                None => pending().await,
            }
        }
    }

    async fn fill_stream_registry(registry: &PullStreamRegistry<u32>) {
        for expected_id in 1..=10 {
            assert_eq!(registry.start(PendingStream).await, Ok(expected_id));
        }
    }

    fn start_pending_transport(
        started: Arc<AtomicBool>,
    ) -> impl Future<Output = Result<PendingStream, ViewerClientError>> {
        std::future::poll_fn(move |_| {
            started.store(true, Ordering::Release);
            std::task::Poll::Ready(Ok(PendingStream))
        })
    }

    #[tokio::test]
    async fn stream_registry_enforces_its_independent_limit() {
        let registry = PullStreamRegistry::new(10);
        fill_stream_registry(&registry).await;

        assert_eq!(
            registry.start(PendingStream).await,
            Err(ViewerClientError::busy())
        );
        registry.cancel(1).await.unwrap();
        assert!(registry.start(PendingStream).await.is_ok());
        registry.cancel_all().await;
    }

    #[tokio::test]
    async fn full_registry_rejects_before_starting_the_transport_stream() {
        let registry = PullStreamRegistry::new(1);
        registry.start(PendingStream).await.unwrap();
        let started = Arc::new(AtomicBool::new(false));
        let transport_started = Arc::clone(&started);

        let result = registry
            .start_with(start_pending_transport(transport_started))
            .await;

        assert_eq!(result, Err(ViewerClientError::busy()));
        assert!(!started.load(Ordering::Acquire));
        registry.cancel_all().await;
    }

    #[tokio::test]
    async fn terminal_stream_response_releases_the_slot() {
        let registry = PullStreamRegistry::new(1);
        let stream_id = registry
            .start(FiniteStream {
                items: VecDeque::from([Ok(Some(7)), Ok(None)]),
            })
            .await
            .unwrap();

        assert_eq!(registry.next_batch(stream_id).await, Ok(vec![7]));
        assert_eq!(registry.next_batch(stream_id).await, Ok(Vec::new()));
        assert_eq!(
            registry.next_batch(stream_id).await,
            Err(ViewerClientError::StreamClosed)
        );
        assert!(
            registry
                .start(FiniteStream {
                    items: VecDeque::new(),
                })
                .await
                .is_ok()
        );
        registry.cancel_all().await;
    }

    #[tokio::test]
    async fn cancelling_a_stream_invalidates_its_id() {
        let registry = PullStreamRegistry::new(1);
        let stream_id = registry.start(PendingStream).await.unwrap();

        registry.cancel(stream_id).await.unwrap();

        assert_eq!(
            registry.next_batch(stream_id).await,
            Err(ViewerClientError::StreamClosed)
        );
        assert_eq!(
            registry.cancel(stream_id).await,
            Err(ViewerClientError::StreamClosed)
        );
    }

    #[tokio::test]
    async fn stream_batch_is_bounded_and_preserves_terminal_order() {
        let registry = PullStreamRegistry::new(1);
        let stream_id = registry
            .start(FiniteStream {
                items: VecDeque::from([
                    Ok(Some(1)),
                    Ok(Some(2)),
                    Ok(Some(3)),
                    Ok(Some(4)),
                    Ok(Some(5)),
                    Err(ViewerClientError::Disconnected),
                ]),
            })
            .await
            .unwrap();

        assert_eq!(registry.next_batch(stream_id).await, Ok(vec![1, 2, 3, 4]));
        assert_eq!(registry.next_batch(stream_id).await, Ok(vec![5]));
        assert_eq!(
            registry.next_batch(stream_id).await,
            Err(ViewerClientError::Disconnected)
        );
        assert_eq!(
            registry.next_batch(stream_id).await,
            Err(ViewerClientError::StreamClosed)
        );
    }

    #[tokio::test]
    async fn sparse_stream_batch_returns_without_waiting_for_another_item() {
        let registry = PullStreamRegistry::new(1);
        let stream_id = registry
            .start(OneThenPendingStream { item: Some(7) })
            .await
            .unwrap();

        let batch =
            tokio::time::timeout(Duration::from_millis(100), registry.next_batch(stream_id))
                .await
                .unwrap();

        assert_eq!(batch, Ok(vec![7]));
        registry.cancel_all().await;
    }
}

viewer_request_command!(
    viewer_discover_project_repositories,
    DiscoverProjectRepositories,
    ProjectDiscovery,
    discover_project_repositories
);
viewer_request_command!(
    viewer_import_project_repositories,
    ImportProjectRepositories,
    Vec<ProjectImportResult>,
    import_project_repositories
);
viewer_request_command!(
    viewer_list_projects,
    ListViewerProjects,
    ViewerProjectPage,
    list_projects
);
viewer_request_command!(
    viewer_get_project_status,
    GetViewerProjectStatus,
    ViewerProjectStatus,
    get_project_status
);
viewer_request_command!(
    viewer_open_project,
    OpenViewerProject,
    OpenViewerProjectOk,
    open_project
);

viewer_request_command!(
    viewer_update_project,
    UpdateViewerProject,
    (),
    update_project
);

viewer_request_command!(
    viewer_set_modified_files,
    SetViewerModifiedFiles,
    (),
    set_modified_files
);

viewer_request_command!(
    viewer_set_tab_pinned,
    SetViewerTabPinned,
    (),
    set_tab_pinned
);
viewer_request_command!(
    viewer_close_other_tabs,
    ViewerTabRequest,
    (),
    close_other_tabs
);

viewer_request_command!(
    viewer_get_file_filters,
    gtl_wire::viewer::ViewerTabRequest,
    gtl_wire::viewer::file_filters::ViewerFileFilters,
    get_file_filters
);
viewer_request_command!(
    viewer_set_file_filters,
    gtl_wire::viewer::file_filters::SetViewerFileFilters,
    (),
    set_file_filters
);

viewer_request_command!(
    viewer_create_push,
    gtl_wire::viewer::push::CreateViewerPush,
    gtl_wire::viewer::push::ViewerPushRequest,
    create_push
);
viewer_request_command!(
    viewer_get_push,
    gtl_wire::viewer::push::ViewerPushRequest,
    gtl_wire::viewer::push::ViewerPushStatus,
    get_push
);
viewer_request_command!(
    viewer_start_push,
    gtl_wire::viewer::push::ViewerPushRequest,
    (),
    start_push
);

viewer_request_command!(
    viewer_get_push_availability,
    gtl_wire::viewer::ViewerViewIdentity,
    gtl_wire::viewer::push::ViewerPushState,
    get_push_availability
);

viewer_request_command!(
    viewer_rename_snapshot,
    RenameViewerSnapshot,
    (),
    rename_snapshot
);

// Tauri's command macro expands to an unreachable fallback arm.
#[allow(clippy::unreachable)]
mod settings_source_command {
    use super::{State, ViewerClientError, ViewerIpcState};

    #[tauri::command]
    pub(crate) async fn viewer_read_settings_file(
        state: State<'_, ViewerIpcState>,
    ) -> Result<String, ViewerClientError> {
        let settings = state.client().await?.get_settings().await?;
        let Some(path) = settings.configuration_path else {
            return Ok(String::new());
        };
        tauri::async_runtime::spawn_blocking(move || {
            use std::io::Read as _;
            let mut source = String::new();
            std::fs::File::open(path)?
                .take(1_048_577)
                .read_to_string(&mut source)?;
            if source.len() > 1_048_576 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Settings source exceeds 1 MiB",
                ));
            }
            Ok(source)
        })
        .await
        .map_err(|error| ViewerClientError::desktop(&error))?
        .map_err(|error| ViewerClientError::desktop(&error))
    }
}

pub(crate) use settings_source_command::viewer_read_settings_file;
