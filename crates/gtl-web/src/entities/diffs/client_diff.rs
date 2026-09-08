#[cfg(feature = "desktop")]
use std::collections::HashSet;

use dioxus::prelude::*;
#[cfg(feature = "desktop")]
use gtl_wire::viewer::{StreamViewerRows, ViewerActiveView, ViewerDiffFileId};
#[cfg(feature = "artifact")]
use gtl_wire::viewer::{ViewerFileRows, ViewerRows};
use gtl_wire::viewer::{ViewerFileSummary, ViewerViewIdentity};

#[cfg(feature = "desktop")]
use super::ViewerRowEvent;
use super::{ViewerSplitRow, ViewerUnifiedRow};
#[cfg(feature = "desktop")]
use crate::shared::{retry_delay::RetryDelay, viewer_client::ViewerClientError};

const CLIENT_LINE_BATCH_SIZE: usize = 64;

#[derive(Debug, Store, Clone, Default, PartialEq, Eq)]
pub(crate) struct ClientDiffRows {
    pub(crate) unified: Vec<Vec<ViewerUnifiedRow>>,
    pub(crate) split: Vec<Vec<ViewerSplitRow>>,
}

impl ClientDiffRows {
    #[cfg(any(feature = "artifact", test))]
    fn append_unified(&mut self, rows: Vec<ViewerUnifiedRow>) {
        self.unified.extend(bounded_batches(rows));
    }

    #[cfg(any(feature = "artifact", test))]
    fn append_split(&mut self, rows: Vec<ViewerSplitRow>) {
        self.split.extend(bounded_batches(rows));
    }
}

fn bounded_batches<Row>(rows: Vec<Row>) -> impl Iterator<Item = Vec<Row>> {
    let mut rows = rows.into_iter();
    std::iter::from_fn(move || {
        let batch = rows
            .by_ref()
            .take(CLIENT_LINE_BATCH_SIZE)
            .collect::<Vec<_>>();
        (!batch.is_empty()).then_some(batch)
    })
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClientDiffFileError {
    Transport(ViewerClientError),
    InvalidResponse,
    Server { message: String, retryable: bool },
}

#[cfg(feature = "desktop")]
impl ClientDiffFileError {
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Transport(ViewerClientError::Unavailable) => {
                "The diff row stream disconnected. Retrying automatically."
            }
            Self::Transport(error) => error.message(),
            Self::InvalidResponse => {
                "The server returned invalid diff rows. Retry this view to load it again."
            }
            Self::Server { message, .. } => message,
        }
    }

    pub(crate) const fn retryable(&self) -> bool {
        match self {
            Self::Transport(_) | Self::InvalidResponse => true,
            Self::Server { retryable, .. } => *retryable,
        }
    }

    const fn retries_automatically(&self) -> bool {
        matches!(self, Self::Transport(ViewerClientError::Unavailable))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClientDiffFileState {
    #[cfg(feature = "desktop")]
    Loading,
    Complete,
    #[cfg(feature = "desktop")]
    Error(ClientDiffFileError),
}

#[cfg(feature = "desktop")]
impl ClientDiffFileState {
    const fn accepts_stream_events(&self) -> bool {
        matches!(self, Self::Loading)
    }
}

#[derive(Debug, Store, Clone, PartialEq, Eq)]
pub(crate) struct ClientDiffFile {
    pub(crate) summary: ViewerFileSummary,
    pub(crate) rows: ClientDiffRows,
    pub(crate) line_number_digits: u32,
    pub(crate) state: ClientDiffFileState,
}

impl ClientDiffFile {
    #[cfg(feature = "desktop")]
    fn loading(summary: ViewerFileSummary) -> Self {
        Self {
            summary,
            rows: ClientDiffRows::default(),
            line_number_digits: 1,
            state: ClientDiffFileState::Loading,
        }
    }
}

#[derive(Debug, Store, Clone, PartialEq, Eq)]
pub(crate) struct ClientDiffWorkspace {
    pub(crate) identity: ViewerViewIdentity,
    pub(crate) files: Vec<ClientDiffFile>,
}

impl ClientDiffWorkspace {
    #[cfg(feature = "desktop")]
    pub(super) fn loading(identity: ViewerViewIdentity, files: Vec<ViewerFileSummary>) -> Self {
        Self {
            identity,
            files: files.into_iter().map(ClientDiffFile::loading).collect(),
        }
    }
}

#[cfg(feature = "artifact")]
pub(crate) fn static_diff_workspace(
    identity: ViewerViewIdentity,
    files: Vec<(ViewerFileSummary, ViewerFileRows)>,
) -> ClientDiffWorkspace {
    let files = files
        .into_iter()
        .map(|(summary, rows)| static_diff_file(summary, rows))
        .collect();
    ClientDiffWorkspace { identity, files }
}

#[cfg(feature = "artifact")]
fn static_diff_file(summary: ViewerFileSummary, rows: ViewerFileRows) -> ClientDiffFile {
    let line_number_digits = rows.line_number_digits;
    let mut projected = ClientDiffRows::default();
    match rows.rows {
        ViewerRows::Unified(rows) => projected.append_unified(rows),
        ViewerRows::Split(rows) => projected.append_split(rows),
    }
    ClientDiffFile {
        summary,
        rows: projected,
        line_number_digits,
        state: ClientDiffFileState::Complete,
    }
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ClientDiffRetryRequest {
    generation: u64,
    identity: Option<ViewerViewIdentity>,
    file: Option<ViewerDiffFileId>,
}

#[cfg(feature = "desktop")]
impl ClientDiffRetryRequest {
    fn request(&mut self, identity: ViewerViewIdentity, file: ViewerDiffFileId) {
        self.generation = self.generation.wrapping_add(1);
        self.identity = Some(identity);
        self.file = Some(file);
    }

    fn requested_file(&self, identity: ViewerViewIdentity) -> Option<ViewerDiffFileId> {
        (self.identity == Some(identity))
            .then(|| self.file.clone())
            .flatten()
    }
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy)]
pub(crate) struct ClientDiffWorkspaceController {
    workspace: Signal<Option<ClientDiffWorkspaceSelection>>,
    row_stream: Resource<()>,
    retry_request: Signal<ClientDiffRetryRequest>,
    identity: Memo<ViewerViewIdentity>,
    cache_key: Memo<super::client_diff_cache::ClientDiffCacheKey>,
}

#[cfg(feature = "desktop")]
#[derive(Clone)]
struct ClientDiffWorkspaceSelection {
    key: super::client_diff_cache::ClientDiffCacheKey,
    workspace: Store<ClientDiffWorkspace>,
}

#[cfg(feature = "desktop")]
impl ClientDiffWorkspaceController {
    pub(crate) fn workspace(self) -> Option<Store<ClientDiffWorkspace>> {
        self.workspace
            .read()
            .as_ref()
            .filter(|selected| selected.key == (self.cache_key)())
            .map(|selected| selected.workspace)
    }

    pub(crate) fn row_stream_active(self) -> bool {
        self.row_stream.state().cloned() == UseResourceState::Pending
    }

    pub(crate) fn retry_file(self, file_id: ViewerDiffFileId) {
        if self.row_stream.pending() {
            return;
        }
        let Some(workspace) = self.workspace() else {
            return;
        };
        let retryable = workspace
            .peek()
            .files
            .iter()
            .find(|file| file.summary.id == file_id)
            .is_some_and(|file| {
                matches!(&file.state, ClientDiffFileState::Error(error) if error.retryable())
            });
        if !retryable {
            return;
        }
        let mut retry_request = self.retry_request;
        retry_request
            .write()
            .request(self.identity.cloned(), file_id);
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn use_client_diff_workspace(
    view: ReadSignal<ViewerActiveView>,
) -> ClientDiffWorkspaceController {
    let identity = use_memo(move || view.read().identity);
    let cache = use_context::<super::client_diff_cache::ClientDiffCache>();
    let viewer = use_context::<crate::app::application_layout::ViewerContext>();
    let cache_key = use_memo(move || super::client_diff_cache::ClientDiffCacheKey {
        server_instance_id: viewer.server_instance_id(),
        content_id: view.read().content_id,
    });
    let workspace = use_signal(|| None::<ClientDiffWorkspaceSelection>);
    let retry_request = use_signal(ClientDiffRetryRequest::default);
    let row_stream = use_resource(move || {
        let identity = identity.cloned();
        let key = cache_key();
        let retry_request = retry_request();
        let requested_file = retry_request.requested_file(identity);
        load_workspace_rows(cache, key, view, workspace, identity, requested_file)
    });

    ClientDiffWorkspaceController {
        workspace,
        row_stream,
        retry_request,
        identity,
        cache_key,
    }
}

#[cfg(feature = "desktop")]
async fn load_workspace_rows(
    cache: super::client_diff_cache::ClientDiffCache,
    key: super::client_diff_cache::ClientDiffCacheKey,
    view: ReadSignal<ViewerActiveView>,
    mut workspace: Signal<Option<ClientDiffWorkspaceSelection>>,
    identity: ViewerViewIdentity,
    requested_file: Option<ViewerDiffFileId>,
) {
    let cached = cache.select(key.server_instance_id.clone(), &view.peek());
    let retained = workspace
        .peek()
        .as_ref()
        .filter(|selected| selected.key == key)
        .map(|selected| selected.workspace);
    let selected = retained.unwrap_or(cached);
    if retained.is_none() {
        workspace.set(Some(ClientDiffWorkspaceSelection {
            key,
            workspace: selected,
        }));
    }
    let file_ids = requested_file.as_ref().map_or_else(
        || {
            selected
                .peek()
                .files
                .iter()
                .map(|file| file.summary.id.clone())
                .collect()
        },
        |file| vec![file.clone()],
    );
    load_rows_from_server(selected, identity, requested_file, file_ids).await;
}

#[cfg(feature = "desktop")]
struct ClientDiffLoad {
    workspace: Store<ClientDiffWorkspace>,
    identity: ViewerViewIdentity,
    files: HashSet<ViewerDiffFileId>,
}

#[cfg(feature = "desktop")]
impl ClientDiffLoad {
    fn is_current(&self) -> bool {
        self.workspace.peek().identity == self.identity
    }

    fn has_active_files(&self) -> bool {
        self.is_current()
            && self
                .files
                .iter()
                .any(|file| self.file_state(file) == Some(ClientDiffFileState::Loading))
    }

    fn file_index(&self, file_id: &ViewerDiffFileId) -> Option<usize> {
        if !self.is_current() || !self.files.contains(file_id) {
            return None;
        }
        self.workspace
            .files()
            .peek()
            .iter()
            .position(|file| &file.summary.id == file_id)
    }

    fn file_state(&self, file_id: &ViewerDiffFileId) -> Option<ClientDiffFileState> {
        let index = self.file_index(file_id)?;
        Some(self.workspace.files().get(index)?.state().peek().clone())
    }

    fn active_file_index(&self, file_id: &ViewerDiffFileId) -> Option<usize> {
        let index = self.file_index(file_id)?;
        self.workspace
            .files()
            .get(index)?
            .state()
            .peek()
            .accepts_stream_events()
            .then_some(index)
    }

    fn fail_active(&self, error: &ClientDiffFileError) {
        for file_id in &self.files {
            self.fail_file(file_id, error);
        }
    }

    fn fail_file(&self, file_id: &ViewerDiffFileId, error: &ClientDiffFileError) {
        let Some(index) = self.active_file_index(file_id) else {
            return;
        };
        let Some(file) = self.workspace.files().get(index) else {
            return;
        };
        file.state().set(ClientDiffFileState::Error(error.clone()));
    }

    fn has_completed_file(&self, file_id: &ViewerDiffFileId) -> bool {
        self.workspace
            .peek()
            .files
            .iter()
            .any(|file| &file.summary.id == file_id && file.state == ClientDiffFileState::Complete)
    }

    fn accept_event(&self, event: ViewerRowEvent) -> bool {
        let event_file = match &event {
            ViewerRowEvent::FileStarted { file }
            | ViewerRowEvent::UnifiedRows { file, .. }
            | ViewerRowEvent::SplitRows { file, .. }
            | ViewerRowEvent::FileFinished { file, .. }
            | ViewerRowEvent::FileFailed { file, .. } => file,
        };
        if !self.files.contains(event_file) {
            return self.has_completed_file(event_file);
        }
        match event {
            ViewerRowEvent::FileStarted { file } => self.accept_file_started(&file),
            ViewerRowEvent::UnifiedRows { file, rows } => self.accept_unified_rows(&file, rows),
            ViewerRowEvent::SplitRows { file, rows } => self.accept_split_rows(&file, rows),
            ViewerRowEvent::FileFinished {
                file,
                line_number_digits,
            } => self.accept_file_finished(&file, line_number_digits),
            ViewerRowEvent::FileFailed {
                file,
                code: _,
                message,
                retryable,
            } => self.accept_file_failed(&file, message, retryable),
        }
    }

    fn accept_file_started(&self, file_id: &ViewerDiffFileId) -> bool {
        let Some(index) = self.active_file_index(file_id) else {
            return false;
        };
        let Some(file) = self.workspace.files().get(index) else {
            return false;
        };
        file.rows().set(ClientDiffRows::default());
        file.line_number_digits().set(1);
        file.state().set(ClientDiffFileState::Loading);
        true
    }

    fn accept_unified_rows(&self, file_id: &ViewerDiffFileId, rows: Vec<ViewerUnifiedRow>) -> bool {
        if self.identity.render_options.layout != gtl_wire::viewer::ViewerDiffLayout::Unified {
            return false;
        }
        let Some(index) = self.active_file_index(file_id) else {
            return false;
        };
        let Some(file) = self.workspace.files().get(index) else {
            return false;
        };
        let mut batches = file.rows().unified();
        for batch in bounded_batches(rows) {
            batches.push(batch);
        }
        true
    }

    fn accept_split_rows(&self, file_id: &ViewerDiffFileId, rows: Vec<ViewerSplitRow>) -> bool {
        if self.identity.render_options.layout != gtl_wire::viewer::ViewerDiffLayout::Split {
            return false;
        }
        let Some(index) = self.active_file_index(file_id) else {
            return false;
        };
        let Some(file) = self.workspace.files().get(index) else {
            return false;
        };
        let mut batches = file.rows().split();
        for batch in bounded_batches(rows) {
            batches.push(batch);
        }
        true
    }

    fn accept_file_finished(&self, file_id: &ViewerDiffFileId, line_number_digits: u32) -> bool {
        let Some(index) = self.active_file_index(file_id) else {
            return false;
        };
        let Some(file) = self.workspace.files().get(index) else {
            return false;
        };
        file.line_number_digits().set(line_number_digits.max(1));
        file.state().set(ClientDiffFileState::Complete);
        true
    }

    fn accept_file_failed(
        &self,
        file_id: &ViewerDiffFileId,
        message: String,
        retryable: bool,
    ) -> bool {
        let Some(index) = self.active_file_index(file_id) else {
            return false;
        };
        let Some(file) = self.workspace.files().get(index) else {
            return false;
        };
        file.state()
            .set(ClientDiffFileState::Error(ClientDiffFileError::Server {
                message,
                retryable,
            }));
        true
    }
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClientDiffStreamResult {
    retry_connection: bool,
    received_rows: bool,
}

#[cfg(feature = "desktop")]
async fn load_rows_from_server(
    workspace: Store<ClientDiffWorkspace>,
    identity: ViewerViewIdentity,
    only_file: Option<ViewerDiffFileId>,
    mut file_ids: Vec<ViewerDiffFileId>,
) {
    let mut retry_delay = RetryDelay::default();
    loop {
        let Some(load) = begin_files(workspace, identity, file_ids) else {
            return;
        };
        let result = read_row_stream(&load, only_file.clone()).await;
        if !result.retry_connection {
            return;
        }
        if result.received_rows {
            retry_delay.reset();
        }
        dioxus_sdk_time::sleep(retry_delay.take_and_advance()).await;
        file_ids = files_waiting_for_connection(workspace, identity, only_file.as_ref());
        if file_ids.is_empty() {
            return;
        }
    }
}

#[cfg(feature = "desktop")]
fn begin_files(
    workspace: Store<ClientDiffWorkspace>,
    identity: ViewerViewIdentity,
    file_ids: Vec<ViewerDiffFileId>,
) -> Option<ClientDiffLoad> {
    if workspace.peek().identity != identity {
        return None;
    }

    let mut selected = HashSet::new();
    for file_id in file_ids {
        let Some(index) = workspace
            .files()
            .peek()
            .iter()
            .position(|file| file.summary.id == file_id)
        else {
            continue;
        };
        let Some(file) = workspace.files().get(index) else {
            continue;
        };
        if *file.state().peek() == ClientDiffFileState::Complete {
            continue;
        }
        file.rows().set(ClientDiffRows::default());
        file.line_number_digits().set(1);
        file.state().set(ClientDiffFileState::Loading);
        selected.insert(file_id);
    }
    (!selected.is_empty()).then_some(ClientDiffLoad {
        workspace,
        identity,
        files: selected,
    })
}

#[cfg(feature = "desktop")]
fn files_waiting_for_connection(
    workspace: Store<ClientDiffWorkspace>,
    identity: ViewerViewIdentity,
    only_file: Option<&ViewerDiffFileId>,
) -> Vec<ViewerDiffFileId> {
    if workspace.peek().identity != identity {
        return Vec::new();
    }
    workspace
        .files()
        .peek()
        .iter()
        .filter(|file| {
            only_file.is_none_or(|file_id| &file.summary.id == file_id)
                && matches!(
                    &file.state,
                    ClientDiffFileState::Error(error) if error.retries_automatically()
                )
        })
        .map(|file| file.summary.id.clone())
        .collect()
}

#[cfg(feature = "desktop")]
async fn read_row_stream(
    load: &ClientDiffLoad,
    only_file: Option<ViewerDiffFileId>,
) -> ClientDiffStreamResult {
    let mut stream = match super::viewer_server::stream_rows(StreamViewerRows {
        identity: load.identity,
        file: only_file,
    })
    .await
    {
        Ok(stream) => stream,
        Err(error) => {
            let retry_connection = error == ViewerClientError::Unavailable;
            load.fail_active(&ClientDiffFileError::Transport(error));
            return ClientDiffStreamResult {
                retry_connection,
                received_rows: false,
            };
        }
    };
    let mut expected_sequence = 0_u64;
    let mut received_rows = false;
    loop {
        if !load.is_current() {
            return ClientDiffStreamResult {
                retry_connection: false,
                received_rows,
            };
        }
        let response = match stream.message().await {
            Ok(Some(response)) => response,
            Ok(None) if !load.has_active_files() => {
                return ClientDiffStreamResult {
                    retry_connection: false,
                    received_rows,
                };
            }
            Ok(None) => {
                load.fail_active(&ClientDiffFileError::Transport(
                    ViewerClientError::Unavailable,
                ));
                return ClientDiffStreamResult {
                    retry_connection: true,
                    received_rows,
                };
            }
            Err(error) => {
                let retry_connection = error == ViewerClientError::Unavailable;
                load.fail_active(&ClientDiffFileError::Transport(error));
                return ClientDiffStreamResult {
                    retry_connection,
                    received_rows,
                };
            }
        };
        if response.identity != load.identity || response.sequence != expected_sequence {
            load.fail_active(&ClientDiffFileError::InvalidResponse);
            return ClientDiffStreamResult {
                retry_connection: false,
                received_rows,
            };
        }
        received_rows |= load.accept_event(response.event);
        let Some(next) = expected_sequence.checked_add(1) else {
            load.fail_active(&ClientDiffFileError::InvalidResponse);
            return ClientDiffStreamResult {
                retry_connection: false,
                received_rows,
            };
        };
        expected_sequence = next;
    }
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use gtl_wire::viewer::{ViewerDiffFileId, ViewerDiffLayout, ViewerSplitRow};

    use super::*;
    #[test]
    fn unavailable_row_streams_retry_automatically_but_invalid_rows_do_not() {
        assert!(
            ClientDiffFileError::Transport(ViewerClientError::Unavailable).retries_automatically()
        );
        assert!(!ClientDiffFileError::InvalidResponse.retries_automatically());
        assert!(
            !ClientDiffFileError::Transport(ViewerClientError::InvalidRequest)
                .retries_automatically()
        );
    }

    #[test]
    fn completed_and_failed_files_reject_later_stream_events() {
        assert!(ClientDiffFileState::Loading.accepts_stream_events());
        assert!(!ClientDiffFileState::Complete.accepts_stream_events());
        assert!(
            !ClientDiffFileState::Error(ClientDiffFileError::InvalidResponse)
                .accepts_stream_events()
        );
    }

    #[test]
    fn resumed_stream_preserves_completed_files_and_rejects_unknown_files()
    -> crate::test_support::TestResult {
        use gtl_models::{
            diffs::DiffLineCount,
            viewer::{ViewerRangeGeneration, ViewerSelectionGeneration},
        };
        use gtl_wire::viewer::{ViewerDiffDensity, ViewerFileStatus, ViewerRenderOptions};

        use crate::test_support::{absolute_file_path, repository_relative_path, viewer_tab_id};

        let identity = ViewerViewIdentity {
            tab_id: viewer_tab_id(1)?,
            range_generation: ViewerRangeGeneration::new(1),
            selection_generation: ViewerSelectionGeneration::default(),
            render_options: ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        };
        let summaries = (0..2)
            .map(|index| {
                Ok(ViewerFileSummary {
                    id: ViewerDiffFileId::for_index(index),
                    path: repository_relative_path(&format!("src/file_{index}.rs"))?,
                    absolute_path: absolute_file_path(format!("/repo/src/file_{index}.rs"))?,
                    anchor_id: format!("file-{index}"),
                    added: DiffLineCount::new(1),
                    removed: DiffLineCount::default(),
                    status: ViewerFileStatus::Added,
                    can_open_in_editor: true,
                    initially_expanded: true,
                })
            })
            .collect::<crate::test_support::TestResult<Vec<_>>>()?;
        let completed_id = summaries[0].id.clone();
        let pending_id = summaries[1].id.clone();
        let mut cached = ClientDiffWorkspace::loading(identity, summaries);
        cached.files[0]
            .rows
            .append_unified(vec![ViewerUnifiedRow::Meta("cached".to_owned())]);
        cached.files[0].state = ClientDiffFileState::Complete;
        cached.files[0].line_number_digits = 3;
        cached.files[1]
            .rows
            .append_unified(vec![ViewerUnifiedRow::Meta("partial".to_owned())]);
        cached.files[1].state = ClientDiffFileState::Error(ClientDiffFileError::Transport(
            ViewerClientError::Unavailable,
        ));
        let completed = cached.files[0].clone();
        let rows = cached.files[0].rows.unified[0].as_ptr();
        let owner = VirtualDom::new(VNode::empty);
        owner.in_scope(ScopeId::ROOT, || -> crate::test_support::TestResult {
            let workspace = Store::new(cached);
            assert_eq!(
                files_waiting_for_connection(workspace, identity, None),
                vec![pending_id.clone()]
            );
            let load = begin_files(
                workspace,
                identity,
                vec![completed_id.clone(), pending_id.clone()],
            )
            .ok_or_else(|| std::io::Error::other("pending file did not resume"))?;
            assert!(load.has_active_files());
            assert!(workspace.peek().files[1].rows.unified.is_empty());

            assert_replayed_file(&load, completed_id, true);
            assert_replayed_file(&load, ViewerDiffFileId::for_index(99), false);

            assert!(load.accept_event(ViewerRowEvent::FileStarted {
                file: pending_id.clone()
            }));
            let resumed_rows = vec![ViewerUnifiedRow::Meta("resumed".to_owned())];
            assert!(load.accept_event(ViewerRowEvent::UnifiedRows {
                file: pending_id.clone(),
                rows: resumed_rows.clone(),
            }));
            assert!(load.accept_event(ViewerRowEvent::FileFinished {
                file: pending_id,
                line_number_digits: 2,
            }));
            assert!(!load.has_active_files());
            let file_ids = workspace
                .peek()
                .files
                .iter()
                .map(|file| file.summary.id.clone())
                .collect();
            assert!(begin_files(workspace, identity, file_ids).is_none());
            assert_eq!(workspace.peek().files[1].rows.unified, vec![resumed_rows]);
            assert_eq!(
                workspace.peek().files[1].state,
                ClientDiffFileState::Complete
            );
            assert_eq!(workspace.peek().files[1].line_number_digits, 2);
            assert_eq!(workspace.peek().files[0], completed);
            assert_eq!(workspace.peek().files[0].rows.unified[0].as_ptr(), rows);
            Ok(())
        })
    }

    fn assert_replayed_file(load: &ClientDiffLoad, file: ViewerDiffFileId, accepted: bool) {
        use gtl_wire::viewer::ViewerFileFailureCode;

        let workspace = load.workspace.peek().clone();
        let rows = load.workspace.peek().files[0].rows.unified[0].as_ptr();
        for event in [
            ViewerRowEvent::FileStarted { file: file.clone() },
            ViewerRowEvent::UnifiedRows {
                file: file.clone(),
                rows: vec![ViewerUnifiedRow::Meta("replayed".to_owned())],
            },
            ViewerRowEvent::SplitRows {
                file: file.clone(),
                rows: vec![ViewerSplitRow::Meta("replayed".to_owned())],
            },
            ViewerRowEvent::FileFinished {
                file: file.clone(),
                line_number_digits: 7,
            },
            ViewerRowEvent::FileFailed {
                file,
                code: ViewerFileFailureCode::ParseFailed,
                message: "replayed failure".to_owned(),
                retryable: true,
            },
        ] {
            assert_eq!(load.accept_event(event), accepted);
            assert_eq!(*load.workspace.peek(), workspace);
            assert_eq!(
                load.workspace.peek().files[0].rows.unified[0].as_ptr(),
                rows
            );
        }
    }

    #[test]
    fn retry_requests_apply_only_to_their_view_identity() -> crate::test_support::TestResult {
        let file = ViewerDiffFileId::for_index(3);
        let current_identity = gtl_wire::viewer::ViewerViewIdentity {
            tab_id: crate::test_support::viewer_tab_id(7)?,
            range_generation: gtl_models::viewer::ViewerRangeGeneration::new(1),
            selection_generation: gtl_models::viewer::ViewerSelectionGeneration::new(2),
            render_options: gtl_wire::viewer::ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density: gtl_wire::viewer::ViewerDiffDensity::Compact,
            },
        };
        let mut request = ClientDiffRetryRequest::default();
        request.request(current_identity, file.clone());
        let next_identity = gtl_wire::viewer::ViewerViewIdentity {
            selection_generation: gtl_models::viewer::ViewerSelectionGeneration::new(3),
            ..current_identity
        };

        assert_eq!(request.requested_file(current_identity), Some(file));
        assert_eq!(request.requested_file(next_identity), None);
        Ok(())
    }

    #[test]
    fn received_rows_are_rebatched_to_the_render_limit() {
        let mut unified = ClientDiffRows::default();
        unified.append_unified(
            (0..129)
                .map(|index| ViewerUnifiedRow::Meta(index.to_string()))
                .collect(),
        );
        let unified_lengths = unified.unified.iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(unified_lengths, vec![64, 64, 1]);

        let mut split = ClientDiffRows::default();
        split.append_split(
            (0..129)
                .map(|index| ViewerSplitRow::Meta(index.to_string()))
                .collect(),
        );
        let split_lengths = split.split.iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(split_lengths, vec![64, 64, 1]);
    }
}
