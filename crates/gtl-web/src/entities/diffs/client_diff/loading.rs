mod response;

use dioxus::{
    core::{Task, spawn_forever},
    prelude::*,
};
use futures_channel::oneshot;

const FETCH_ROWS: usize = super::CLIENT_LINE_BATCH_SIZE;
const FETCH_WINDOWS: usize = FETCH_ROWS / super::CLIENT_LINE_BATCH_SIZE;
const FETCH_RETRIES_MAX: usize = 4;
use gtl_wire::viewer::{ViewerActiveView, ViewerDiffFileId, ViewerViewIdentity};

use super::{
    ClientDiffFileError, ClientDiffFileState, ClientDiffFileStoreExt, ClientDiffWorkspace,
    ClientDiffWorkspaceStoreExt,
};
use crate::{
    entities::diffs::client_diff_cache::{ClientDiffCache, ClientDiffCacheKey, retain_windows},
    shared::retry_delay::RetryDelay,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ClientDiffWindow {
    pub(crate) file: usize,
    pub(crate) batch: usize,
}

pub(in crate::entities::diffs) struct ClientDiffFetch {
    identity: ViewerViewIdentity,
    window: ClientDiffWindow,
    task: Option<Task>,
    reply: Option<oneshot::Sender<bool>>,
    pub(in crate::entities::diffs) content: ClientDiffCacheKey,
    network_active: bool,
}

impl Drop for ClientDiffFetch {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.cancel();
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RowDemand {
    identity: Option<ViewerViewIdentity>,
    windows: Vec<ClientDiffWindow>,
}

#[derive(Clone)]
struct WorkspaceSelection {
    key: ClientDiffCacheKey,
    workspace: Store<ClientDiffWorkspace>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ClientDiffWorkspaceController {
    workspace: Signal<Option<WorkspaceSelection>>,
    demand: Signal<RowDemand>,
    stream: Resource<()>,
    key: Memo<ClientDiffCacheKey>,
    cache: ClientDiffCache,
    identity: Memo<ViewerViewIdentity>,
}

impl ClientDiffWorkspaceController {
    pub(crate) fn workspace(self) -> Option<Store<ClientDiffWorkspace>> {
        self.workspace
            .read()
            .as_ref()
            .filter(|selected| selected.key == (self.key)())
            .map(|selected| selected.workspace)
    }

    pub(crate) fn row_stream_active(self) -> bool {
        let identity = (self.identity)();
        self.cache
            .requests
            .read()
            .iter()
            .any(|fetch| fetch.identity == identity && fetch.network_active)
    }

    pub(crate) fn request_windows(
        mut self,
        identity: ViewerViewIdentity,
        windows: Vec<ClientDiffWindow>,
    ) {
        let demand = RowDemand {
            identity: Some(identity),
            windows,
        };
        if *self.demand.peek() != demand {
            self.demand.set(demand);
        }
    }

    pub(crate) fn retry_file(mut self, file_id: &ViewerDiffFileId) {
        let Some(workspace) = self.workspace() else {
            return;
        };
        let retryable = workspace.peek().files.iter().any(|file| {
            &file.summary.id == file_id
                && matches!(&file.state,
                ClientDiffFileState::Error(error) if error.retryable())
        });
        if retryable {
            self.stream.restart();
        }
    }
}

pub(crate) fn use_client_diff_workspace(
    view: ReadSignal<ViewerActiveView>,
) -> ClientDiffWorkspaceController {
    let identity = use_memo(move || view.read().identity);
    let source = use_memo(move || view.read().row_source);
    let summaries = use_memo(move || view.read().files.clone());
    let cache = use_context::<ClientDiffCache>();
    let viewer = use_context::<crate::app::application_layout::ViewerContext>();
    let key = use_memo(move || ClientDiffCacheKey {
        server_instance_id: viewer.server_instance_id(),
        content_id: view.read().content_id,
    });
    let workspace = use_signal(|| None::<WorkspaceSelection>);
    let demand = use_signal(RowDemand::default);
    let stream = use_resource(move || {
        let identity = identity();
        let key = key();
        let demand = demand();
        let _source = source();
        let _summaries = summaries.read();
        load_workspace(cache, key, view, workspace, identity, demand)
    });
    ClientDiffWorkspaceController {
        workspace,
        demand,
        stream,
        key,
        cache,
        identity,
    }
}

async fn load_workspace(
    cache: ClientDiffCache,
    key: ClientDiffCacheKey,
    view: ReadSignal<ViewerActiveView>,
    mut selected: Signal<Option<WorkspaceSelection>>,
    identity: ViewerViewIdentity,
    demand: RowDemand,
) {
    if view.peek().row_source != gtl_wire::viewer::ViewerRowSourceState::Ready {
        if selected.peek().is_some() {
            selected.set(None);
        }
        return;
    }
    let workspace = cache.select(key.server_instance_id.clone(), &view.peek());
    if selected
        .peek()
        .as_ref()
        .is_none_or(|selected| selected.key != key)
    {
        selected.set(Some(WorkspaceSelection {
            key: key.clone(),
            workspace,
        }));
    }
    if demand.identity != Some(identity) {
        return;
    }
    for window in demand.windows.iter().rev() {
        cache.touch_window(&key, *window);
    }
    for (priority, window) in demand.windows.iter().copied().enumerate() {
        if workspace.peek().identity != identity {
            return;
        }
        if cache.contains_window(&key, window) {
            continue;
        }
        if !load_window(cache, &key, identity, window, &demand.windows[..priority]).await {
            return;
        }
    }
}

async fn load_window(
    mut cache: ClientDiffCache,
    key: &ClientDiffCacheKey,
    identity: ViewerViewIdentity,
    window: ClientDiffWindow,
    protected: &[ClientDiffWindow],
) -> bool {
    let window = ClientDiffWindow {
        batch: window.batch / FETCH_WINDOWS * FETCH_WINDOWS,
        ..window
    };
    let (reply, response) = oneshot::channel();
    let previous = {
        let mut requests = cache.requests.write();
        requests
            .iter()
            .position(|fetch| fetch.identity.tab_id == identity.tab_id)
            .and_then(|index| requests.remove(index))
    };
    if let Some(mut previous) = previous {
        if previous.identity == identity && previous.window == window {
            previous.reply = Some(reply);
            cache.requests.write().push_back(previous);
            return response.await.unwrap_or(false);
        }
        drop(previous);
    }
    if cache.requests.peek().len() == gtl_wire::viewer::VIEWER_ROW_SESSIONS_MAX {
        let evicted = cache.requests.write().pop_front();
        drop(evicted);
    }
    let key = key.clone();
    let protected = protected.to_vec();
    let content = key.clone();
    let task = spawn_forever(async move {
        let success = fetch_windows(cache, &key, identity, window, &protected).await;
        let entry = {
            let mut requests = cache.requests.write();
            requests
                .iter()
                .position(|fetch| fetch.identity == identity && fetch.window == window)
                .and_then(|index| requests.remove(index))
        };
        let Some(mut entry) = entry else {
            return;
        };
        entry.task = None;
        if let Some(reply) = entry.reply.take() {
            let _ = reply.send(success);
        }
    });
    cache.requests.write().push_back(ClientDiffFetch {
        identity,
        window,
        task: Some(task),
        reply: Some(reply),
        content,
        network_active: false,
    });
    response.await.unwrap_or(false)
}

async fn fetch_windows(
    mut cache: ClientDiffCache,
    key: &ClientDiffCacheKey,
    identity: ViewerViewIdentity,
    window: ClientDiffWindow,
    protected: &[ClientDiffWindow],
) -> bool {
    let workspace = cache.workspace(key);
    let Some(file) = workspace.files().get(window.file) else {
        return false;
    };
    let summary = file.clone().summary();
    let mut state = file.state();
    let mut retry = RetryDelay::default();
    for attempt in 0..=FETCH_RETRIES_MAX {
        if *state.peek() != ClientDiffFileState::Loading {
            state.set(ClientDiffFileState::Loading);
        }
        let Some(request) = response::WindowRequest::new(identity, window, &summary.peek()) else {
            return false;
        };
        if let Some(fetch) = cache
            .requests
            .write()
            .iter_mut()
            .find(|fetch| fetch.identity == identity)
        {
            fetch.network_active = true;
        }
        let result = response::read(request).await;
        if let Some(fetch) = cache
            .requests
            .write()
            .iter_mut()
            .find(|fetch| fetch.identity == identity)
        {
            fetch.network_active = false;
        }
        let result =
            result.and_then(|windows| retain_windows(cache, key, window, windows, protected));
        let error = match result {
            Ok(retained) => {
                state.set(ClientDiffFileState::Complete);
                return retained;
            }
            Err(error) => error,
        };
        let awaiting = cache.requests.peek().iter().any(|fetch| {
            fetch.identity == identity
                && fetch
                    .reply
                    .as_ref()
                    .is_some_and(|reply| !reply.is_canceled())
        });
        if !error.retries_automatically() || attempt == FETCH_RETRIES_MAX || !awaiting {
            state.set(ClientDiffFileState::Error(error));
            return false;
        }
        dioxus_sdk_time::sleep(retry.take_and_advance()).await;
    }
    false
}

pub(in crate::entities::diffs) struct LoadedRowWindow {
    pub(in crate::entities::diffs) rows: gtl_wire::viewer::ViewerRows,
    pub(in crate::entities::diffs) line_number_digits: u32,
}

pub(in crate::entities::diffs) fn window_too_large() -> ClientDiffFileError {
    ClientDiffFileError::Server {
        failure: gtl_models::failure::ViewerFailure::RangeTooLarge.into(),
        retryable: false,
    }
}
