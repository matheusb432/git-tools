#[cfg(feature = "desktop")]
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "desktop")]
use dioxus::prelude::*;
#[cfg(feature = "desktop")]
use gtl_wire::viewer::{StreamViewerRows, ViewerDiffFileId};
use gtl_wire::viewer::{ViewerDiffLayout, ViewerFileSummary, ViewerViewIdentity};
#[cfg(feature = "artifact")]
use gtl_wire::viewer::{ViewerFileRows, ViewerRows};

#[cfg(feature = "desktop")]
use super::ViewerRowEvent;
use super::{ViewerSplitRow, ViewerUnifiedRow};
#[cfg(feature = "desktop")]
use crate::shared::{retry_delay::RetryDelay, viewer_client::ViewerClientError};

const CLIENT_LINE_BATCH_SIZE: usize = 64;

/// Orders asynchronous page-loading requests independently from viewer identity generations.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ClientDiffRequestTicket(u64);

#[cfg(feature = "desktop")]
impl ClientDiffRequestTicket {
    #[must_use]
    const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClientDiffRows {
    Unified(Vec<Arc<Vec<ViewerUnifiedRow>>>),
    Split(Vec<Arc<Vec<ViewerSplitRow>>>),
}

impl ClientDiffRows {
    fn new(layout: ViewerDiffLayout) -> Self {
        match layout {
            ViewerDiffLayout::Unified => Self::Unified(Vec::new()),
            ViewerDiffLayout::Split => Self::Split(Vec::new()),
        }
    }

    fn append_unified(&mut self, rows: Vec<ViewerUnifiedRow>) {
        if let Self::Unified(batches) = self {
            append_bounded_batches(batches, rows);
        }
    }

    fn append_split(&mut self, rows: Vec<ViewerSplitRow>) {
        if let Self::Split(batches) = self {
            append_bounded_batches(batches, rows);
        }
    }
}

fn append_bounded_batches<Row>(batches: &mut Vec<Arc<Vec<Row>>>, rows: Vec<Row>) {
    let mut rows = rows.into_iter();
    loop {
        let batch = rows
            .by_ref()
            .take(CLIENT_LINE_BATCH_SIZE)
            .collect::<Vec<_>>();
        if batch.is_empty() {
            return;
        }
        batches.push(Arc::new(batch));
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientDiffFile {
    pub(crate) summary: ViewerFileSummary,
    pub(crate) rows: ClientDiffRows,
    pub(crate) line_number_digits: u32,
    pub(crate) state: ClientDiffFileState,
}

impl ClientDiffFile {
    #[cfg(feature = "desktop")]
    fn loading(summary: ViewerFileSummary, layout: ViewerDiffLayout) -> Self {
        Self {
            summary,
            rows: ClientDiffRows::new(layout),
            line_number_digits: 1,
            state: ClientDiffFileState::Loading,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientDiffWorkspace {
    pub(crate) identity: ViewerViewIdentity,
    pub(crate) files: Vec<ClientDiffFile>,
}

impl ClientDiffWorkspace {
    #[cfg(feature = "desktop")]
    fn loading(identity: ViewerViewIdentity, files: Vec<ViewerFileSummary>) -> Self {
        Self {
            identity,
            files: files
                .into_iter()
                .map(|file| ClientDiffFile::loading(file, identity.render_options.layout))
                .collect(),
        }
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn is_loading(&self) -> bool {
        self.files
            .iter()
            .any(|file| file.state == ClientDiffFileState::Loading)
    }
}

#[cfg(feature = "artifact")]
pub(crate) fn static_diff_workspace(
    identity: ViewerViewIdentity,
    files: Vec<(ViewerFileSummary, ViewerFileRows)>,
) -> ClientDiffWorkspace {
    let files = files
        .into_iter()
        .map(|(summary, rows)| static_diff_file(summary, rows, identity.render_options.layout))
        .collect();
    ClientDiffWorkspace { identity, files }
}

#[cfg(feature = "artifact")]
fn static_diff_file(
    summary: ViewerFileSummary,
    rows: ViewerFileRows,
    layout: ViewerDiffLayout,
) -> ClientDiffFile {
    let line_number_digits = rows.line_number_digits;
    let mut projected = ClientDiffRows::new(layout);
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
#[derive(Debug, Default)]
struct ClientDiffAttempts {
    next: ClientDiffRequestTicket,
    active: HashMap<ViewerDiffFileId, ClientDiffRequestTicket>,
}

#[cfg(feature = "desktop")]
impl ClientDiffAttempts {
    fn begin(&mut self, file: ViewerDiffFileId) -> ClientDiffRequestTicket {
        self.next = self.next.next();
        self.active.insert(file, self.next);
        self.next
    }

    fn is_current(&self, file: &ViewerDiffFileId, ticket: ClientDiffRequestTicket) -> bool {
        self.active.get(file) == Some(&ticket)
    }

    fn finish(&mut self, file: &ViewerDiffFileId, ticket: ClientDiffRequestTicket) {
        if self.is_current(file, ticket) {
            self.active.remove(file);
        }
    }
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy)]
struct ClientDiffSession {
    workspace: Signal<ClientDiffWorkspace>,
    generation: Signal<ClientDiffRequestTicket>,
    request_generation: ClientDiffRequestTicket,
    attempts: Signal<ClientDiffAttempts>,
    row_stream_active: Signal<bool>,
    identity: ViewerViewIdentity,
}

#[cfg(feature = "desktop")]
impl ClientDiffSession {
    fn is_current(self) -> bool {
        *self.generation.peek() == self.request_generation
            && self.workspace.peek().identity == self.identity
    }

    fn begin_files(self, file_ids: Vec<ViewerDiffFileId>) -> Option<ClientDiffLoad> {
        if !self.is_current() {
            return None;
        }

        let mut selected = Vec::new();
        {
            let mut workspace = self.workspace;
            let mut current = workspace.write();
            for file_id in file_ids {
                let Some(file) = current
                    .files
                    .iter_mut()
                    .find(|file| file.summary.id == file_id)
                else {
                    continue;
                };
                if file.state == ClientDiffFileState::Complete {
                    continue;
                }
                file.rows = ClientDiffRows::new(self.identity.render_options.layout);
                file.line_number_digits = 1;
                file.state = ClientDiffFileState::Loading;
                selected.push(file_id);
            }
        }
        if selected.is_empty() {
            return None;
        }

        let tickets = {
            let mut attempts = self.attempts;
            let mut attempts = attempts.write();
            selected
                .into_iter()
                .map(|file| {
                    let ticket = attempts.begin(file.clone());
                    (file, ticket)
                })
                .collect()
        };
        Some(ClientDiffLoad {
            session: self,
            tickets: Arc::new(tickets),
        })
    }

    fn files_waiting_for_connection(
        self,
        only_file: Option<&ViewerDiffFileId>,
    ) -> Vec<ViewerDiffFileId> {
        if !self.is_current() {
            return Vec::new();
        }
        self.workspace
            .peek()
            .files
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

    fn start_row_stream(mut self) -> bool {
        if !self.is_current() || *self.row_stream_active.peek() {
            return false;
        }
        self.row_stream_active.set(true);
        true
    }

    fn finish_row_stream(mut self) {
        if self.is_current() {
            self.row_stream_active.set(false);
        }
    }
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy)]
pub(crate) struct ClientDiffWorkspaceController {
    session: ClientDiffSession,
}

#[cfg(feature = "desktop")]
impl ClientDiffWorkspaceController {
    pub(crate) fn read(self) -> ClientDiffWorkspace {
        (self.session.workspace)()
    }

    pub(crate) fn row_stream_active(self) -> bool {
        (self.session.row_stream_active)()
    }

    pub(crate) fn retry_file(self, file_id: ViewerDiffFileId) {
        if self.row_stream_active() {
            return;
        }
        let retryable = self
            .session
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
        spawn(load_rows_with_retry(
            self.session,
            Some(file_id.clone()),
            vec![file_id],
        ));
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn use_client_diff_workspace(
    identity: ViewerViewIdentity,
    files: &[ViewerFileSummary],
) -> ClientDiffWorkspaceController {
    let files = files.to_owned();
    let initial_files = files.clone();
    let mut workspace = use_signal(move || ClientDiffWorkspace::loading(identity, initial_files));
    let mut generation = use_signal(ClientDiffRequestTicket::default);
    let mut attempts = use_signal(ClientDiffAttempts::default);
    let mut row_stream_active = use_signal(|| false);
    let request_generation = generation();
    let controller = ClientDiffWorkspaceController {
        session: ClientDiffSession {
            workspace,
            generation,
            request_generation,
            attempts,
            row_stream_active,
            identity,
        },
    };

    use_effect(use_reactive(
        (&identity, &files),
        move |(identity, files)| {
            let request_generation = {
                let mut current = generation.write();
                let next = current.next();
                *current = next;
                next
            };
            attempts.set(ClientDiffAttempts::default());
            row_stream_active.set(false);
            workspace.set(ClientDiffWorkspace::loading(identity, files.clone()));
            let session = ClientDiffSession {
                workspace,
                generation,
                request_generation,
                attempts,
                row_stream_active,
                identity,
            };
            let file_ids = files.into_iter().map(|file| file.id).collect();
            spawn(load_rows_with_retry(session, None, file_ids));
        },
    ));

    controller
}

#[derive(Clone)]
#[cfg(feature = "desktop")]
struct ClientDiffLoad {
    session: ClientDiffSession,
    tickets: Arc<HashMap<ViewerDiffFileId, ClientDiffRequestTicket>>,
}

#[cfg(feature = "desktop")]
impl ClientDiffLoad {
    fn is_current(&self) -> bool {
        self.session.is_current()
    }

    fn has_active_files(&self) -> bool {
        self.is_current()
            && self
                .tickets
                .iter()
                .any(|(file, ticket)| self.session.attempts.peek().is_current(file, *ticket))
    }

    fn update_file(
        &self,
        file_id: &ViewerDiffFileId,
        update: impl FnOnce(&mut ClientDiffFile),
    ) -> bool {
        let Some(ticket) = self.tickets.get(file_id).copied() else {
            return false;
        };
        if !self.is_current() || !self.session.attempts.peek().is_current(file_id, ticket) {
            return false;
        }
        let mut workspace = self.session.workspace;
        let mut current = workspace.write();
        let Some(file) = current
            .files
            .iter_mut()
            .find(|file| &file.summary.id == file_id)
        else {
            return false;
        };
        update(file);
        true
    }

    fn finish_file(&self, file_id: &ViewerDiffFileId) {
        let Some(ticket) = self.tickets.get(file_id).copied() else {
            return;
        };
        let mut attempts = self.session.attempts;
        attempts.write().finish(file_id, ticket);
    }

    fn fail_active(&self, error: &ClientDiffFileError) {
        for (file_id, ticket) in self.tickets.iter() {
            if !self.session.attempts.peek().is_current(file_id, *ticket) {
                continue;
            }
            if self.update_file(file_id, |file| {
                file.state = ClientDiffFileState::Error(error.clone());
            }) {
                self.finish_file(file_id);
            }
        }
    }

    fn accept_event(&self, event: ViewerRowEvent) -> bool {
        match event {
            ViewerRowEvent::FileStarted { file } => self.update_file(&file, |file| {
                file.rows = ClientDiffRows::new(self.session.identity.render_options.layout);
                file.line_number_digits = 1;
                file.state = ClientDiffFileState::Loading;
            }),
            ViewerRowEvent::UnifiedRows { file, rows } => self.update_file(&file, |file| {
                file.rows.append_unified(rows);
            }),
            ViewerRowEvent::SplitRows { file, rows } => self.update_file(&file, |file| {
                file.rows.append_split(rows);
            }),
            ViewerRowEvent::FileFinished {
                file,
                line_number_digits,
            } => {
                let accepted = self.update_file(&file, |file| {
                    file.line_number_digits = line_number_digits.max(1);
                    file.state = ClientDiffFileState::Complete;
                });
                if accepted {
                    self.finish_file(&file);
                }
                accepted
            }
            ViewerRowEvent::FileFailed {
                file,
                code: _,
                message,
                retryable,
            } => {
                let accepted = self.update_file(&file, |file| {
                    file.state = ClientDiffFileState::Error(ClientDiffFileError::Server {
                        message,
                        retryable,
                    });
                });
                if accepted {
                    self.finish_file(&file);
                }
                accepted
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
async fn load_rows_with_retry(
    session: ClientDiffSession,
    only_file: Option<ViewerDiffFileId>,
    file_ids: Vec<ViewerDiffFileId>,
) {
    if !session.start_row_stream() {
        return;
    }
    load_rows_from_server(session, only_file, file_ids).await;
    session.finish_row_stream();
}

#[cfg(feature = "desktop")]
async fn load_rows_from_server(
    session: ClientDiffSession,
    only_file: Option<ViewerDiffFileId>,
    mut file_ids: Vec<ViewerDiffFileId>,
) {
    let mut retry_delay = RetryDelay::default();
    loop {
        let Some(load) = session.begin_files(file_ids) else {
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
        file_ids = session.files_waiting_for_connection(only_file.as_ref());
        if file_ids.is_empty() {
            return;
        }
    }
}

#[cfg(feature = "desktop")]
async fn read_row_stream(
    load: &ClientDiffLoad,
    only_file: Option<ViewerDiffFileId>,
) -> ClientDiffStreamResult {
    let mut stream = match super::viewer_server::stream_rows(StreamViewerRows {
        identity: load.session.identity,
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
        if response.identity != load.session.identity || response.sequence != expected_sequence {
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
    use gtl_wire::viewer::{ViewerDiffFileId, ViewerSplitRow};

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
    fn finishing_an_old_file_attempt_does_not_cancel_its_replacement() {
        let file = ViewerDiffFileId::for_index(3);
        let mut attempts = ClientDiffAttempts::default();
        let old = attempts.begin(file.clone());
        let current = attempts.begin(file.clone());

        attempts.finish(&file, old);

        assert!(attempts.is_current(&file, current));
    }

    #[test]
    fn received_rows_are_rebatched_to_the_render_limit() {
        let mut unified = ClientDiffRows::Unified(Vec::new());
        unified.append_unified(
            (0..129)
                .map(|index| ViewerUnifiedRow::Meta(index.to_string()))
                .collect(),
        );
        let unified_lengths = match unified {
            ClientDiffRows::Unified(unified) => {
                unified.iter().map(|batch| batch.len()).collect::<Vec<_>>()
            }
            ClientDiffRows::Split(_) => Vec::new(),
        };
        assert_eq!(unified_lengths, vec![64, 64, 1]);

        let mut split = ClientDiffRows::Split(Vec::new());
        split.append_split(
            (0..129)
                .map(|index| ViewerSplitRow::Meta(index.to_string()))
                .collect(),
        );
        let split_lengths = match split {
            ClientDiffRows::Split(split) => {
                split.iter().map(|batch| batch.len()).collect::<Vec<_>>()
            }
            ClientDiffRows::Unified(_) => Vec::new(),
        };
        assert_eq!(split_lengths, vec![64, 64, 1]);
    }
}
