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
    fn loading(identity: ViewerViewIdentity, files: Vec<ViewerFileSummary>) -> Self {
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
    workspace: Store<ClientDiffWorkspace>,
    row_stream: Resource<()>,
    retry_request: Signal<ClientDiffRetryRequest>,
    identity: Memo<ViewerViewIdentity>,
}

#[cfg(feature = "desktop")]
impl ClientDiffWorkspaceController {
    pub(crate) const fn workspace(self) -> Store<ClientDiffWorkspace> {
        self.workspace
    }

    pub(crate) fn row_stream_active(self) -> bool {
        self.row_stream.state().cloned() == UseResourceState::Pending
    }

    pub(crate) fn retry_file(self, file_id: ViewerDiffFileId) {
        if self.row_stream.pending() {
            return;
        }
        let retryable = self
            .workspace
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
    let mut workspace = use_store(move || {
        let view = view.peek();
        ClientDiffWorkspace::loading(view.identity, view.files.clone())
    });
    let retry_request = use_signal(ClientDiffRetryRequest::default);
    let row_stream = use_resource(move || {
        let identity = identity.cloned();
        let retry_request = retry_request();
        let requested_file = retry_request.requested_file(identity);
        let (only_file, file_ids) = requested_file.map_or_else(
            || {
                let files = view.peek().files.clone();
                let file_ids = files.iter().map(|file| file.id.clone()).collect();
                workspace.set(ClientDiffWorkspace::loading(identity, files));
                (None, file_ids)
            },
            |file| (Some(file.clone()), vec![file]),
        );

        async move {
            load_rows_from_server(workspace, identity, only_file, file_ids).await;
        }
    });

    ClientDiffWorkspaceController {
        workspace,
        row_stream,
        retry_request,
        identity,
    }
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
            if let Some(index) = self.active_file_index(file_id)
                && let Some(file) = self.workspace.files().get(index)
            {
                file.state().set(ClientDiffFileState::Error(error.clone()));
            }
        }
    }

    fn accept_event(&self, event: ViewerRowEvent) -> bool {
        match event {
            ViewerRowEvent::FileStarted { file } => {
                let Some(index) = self.active_file_index(&file) else {
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
            ViewerRowEvent::UnifiedRows { file, rows } => {
                if self.identity.render_options.layout
                    != gtl_wire::viewer::ViewerDiffLayout::Unified
                {
                    return false;
                }
                let Some(index) = self.active_file_index(&file) else {
                    return false;
                };
                let Some(file) = self.workspace.files().get(index) else {
                    return false;
                };
                let mut batches = file.rows().unified();
                batches.write().extend(bounded_batches(rows));
                true
            }
            ViewerRowEvent::SplitRows { file, rows } => {
                if self.identity.render_options.layout != gtl_wire::viewer::ViewerDiffLayout::Split
                {
                    return false;
                }
                let Some(index) = self.active_file_index(&file) else {
                    return false;
                };
                let Some(file) = self.workspace.files().get(index) else {
                    return false;
                };
                let mut batches = file.rows().split();
                batches.write().extend(bounded_batches(rows));
                true
            }
            ViewerRowEvent::FileFinished {
                file,
                line_number_digits,
            } => {
                let Some(index) = self.active_file_index(&file) else {
                    return false;
                };
                let Some(file) = self.workspace.files().get(index) else {
                    return false;
                };
                file.line_number_digits().set(line_number_digits.max(1));
                file.state().set(ClientDiffFileState::Complete);
                true
            }
            ViewerRowEvent::FileFailed {
                file,
                code: _,
                message,
                retryable,
            } => {
                let Some(index) = self.active_file_index(&file) else {
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
