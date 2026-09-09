mod response;

use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerActiveView, ViewerDiffFileId, ViewerViewIdentity};

use super::{
    ClientDiffFileError, ClientDiffFileState, ClientDiffFileStoreExt, ClientDiffWorkspace,
    ClientDiffWorkspaceStoreExt,
};
use crate::{
    entities::diffs::client_diff_cache::{ClientDiffCache, ClientDiffCacheKey},
    shared::retry_delay::RetryDelay,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ClientDiffWindow {
    pub(crate) file: usize,
    pub(crate) batch: usize,
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
        self.stream.state().cloned() == UseResourceState::Pending
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
        load_workspace(cache, key, view, workspace, identity, demand)
    });
    ClientDiffWorkspaceController {
        workspace,
        demand,
        stream,
        key,
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
        if !load_window(
            cache,
            &key,
            workspace,
            identity,
            window,
            &demand.windows[..priority],
        )
        .await
        {
            return;
        }
    }
}

async fn load_window(
    cache: ClientDiffCache,
    key: &ClientDiffCacheKey,
    workspace: Store<ClientDiffWorkspace>,
    identity: ViewerViewIdentity,
    window: ClientDiffWindow,
    protected: &[ClientDiffWindow],
) -> bool {
    let Some(file) = workspace.files().get(window.file) else {
        return false;
    };
    let mut retry = RetryDelay::default();
    loop {
        if workspace.peek().identity != identity {
            return false;
        }
        if *file.state().peek() != ClientDiffFileState::Loading {
            file.state().set(ClientDiffFileState::Loading);
        }
        let request = response::WindowRequest::new(identity, window, &file.summary().peek());
        let result = match request {
            Some(request) => response::read(request).await,
            None => return false,
        };
        if workspace.peek().identity != identity {
            return false;
        }
        let result = result.and_then(|rows| cache.retain_window(key, window, rows, protected));
        let reconnect = match result {
            Ok(true) => {
                file.state().set(ClientDiffFileState::Complete);
                return true;
            }
            Ok(false) => return false,
            Err(error) => {
                let reconnect = error.retries_automatically();
                file.state().set(ClientDiffFileState::Error(error));
                reconnect
            }
        };
        if !reconnect {
            return false;
        }
        dioxus_sdk_time::sleep(retry.take_and_advance()).await;
    }
}

pub(in crate::entities::diffs) struct LoadedRowWindow {
    pub(in crate::entities::diffs) rows: gtl_wire::viewer::ViewerRows,
    pub(in crate::entities::diffs) line_number_digits: u32,
}

pub(in crate::entities::diffs) fn window_too_large() -> ClientDiffFileError {
    ClientDiffFileError::Server {
        message: "This section contains too much text to display.".to_owned(),
        retryable: false,
    }
}
