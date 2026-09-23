use std::collections::VecDeque;

use gtl_models::viewer::ViewerTabId;
use gtl_wire::{
    proto, v1,
    viewer::{StreamViewerRows, VIEWER_ROW_SESSIONS_MAX},
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::{ViewerClientError, ViewerRowStream, decode_status};

type Client = v1::viewer_service_client::ViewerServiceClient<tonic::transport::Channel>;
type RowResult = Result<v1::StreamViewerRowsResponse, ViewerClientError>;

struct Demand {
    request: StreamViewerRows,
    rows: mpsc::Sender<RowResult>,
}

struct Session {
    tab: ViewerTabId,
    demands: mpsc::Sender<Demand>,
    task: tokio::task::AbortHandle,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
pub(super) struct RowSessions {
    entries: VecDeque<Session>,
}

impl RowSessions {
    pub(super) fn request(
        &mut self,
        client: Client,
        request: StreamViewerRows,
    ) -> Result<ViewerRowStream, ViewerClientError> {
        self.entries.retain(|entry| !entry.demands.is_closed());
        let tab = request.identity.tab_id;
        let session = self
            .entries
            .iter()
            .position(|entry| entry.tab == tab)
            .and_then(|index| self.entries.remove(index))
            .unwrap_or_else(|| {
                if self.entries.len() == VIEWER_ROW_SESSIONS_MAX {
                    self.entries.pop_front();
                }
                let (demands, receiver) = mpsc::channel(1);
                let task = tokio::spawn(run(client, receiver));
                Session {
                    tab,
                    demands,
                    task: task.abort_handle(),
                }
            });
        let (rows, receiver) = mpsc::channel(2);
        let result =
            session
                .demands
                .try_send(Demand { request, rows })
                .map_err(|error| match error {
                    mpsc::error::TrySendError::Full(_) => ViewerClientError::ResourceExhausted,
                    mpsc::error::TrySendError::Closed(_) => ViewerClientError::Unavailable,
                });
        self.entries.push_back(session);
        result?;
        Ok(ViewerRowStream {
            stream: super::RowResponseStream::Window(receiver),
        })
    }
}

async fn run(mut client: Client, mut demands: mpsc::Receiver<Demand>) {
    let Some(first) = demands.recv().await else {
        return;
    };
    let (requests, receiver) = mpsc::channel(1);
    let mut request_id = 1;
    if requests
        .send(encode_demand(request_id, first.request))
        .await
        .is_err()
    {
        return;
    }
    let response = client
        .stream_viewer_row_session(ReceiverStream::new(receiver))
        .await;
    let mut stream = match response {
        Ok(response) => response.into_inner(),
        Err(error) => {
            let _ = first.rows.send(Err(decode_status(&error))).await;
            return;
        }
    };
    let mut current = Some(first.rows);
    let mut pending = None;
    let mut deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let destination = current.clone();
        tokio::select! {
            biased;
            () = tokio::time::sleep_until(deadline), if current.is_some() => {
                if let Some(rows) = current.take() { let _ = rows.try_send(Err(ViewerClientError::Unavailable)); }
                return;
            }
            demand = demands.recv() => {
                let Some(demand) = demand else { return; };
                let Some(next) = request_id.checked_add(1) else {
                    let _ = demand.rows.send(Err(ViewerClientError::ResourceExhausted)).await;
                    return;
                };
                request_id = next;
                deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
                current = Some(demand.rows);
                pending = None;
                if requests.send(encode_demand(request_id, demand.request)).await.is_err() { return; }
            }
            permit = async move { match destination { Some(rows) => rows.reserve_owned().await, None => std::future::pending().await } }, if pending.is_some() => {
                if let Ok(permit) = permit {
                    if let Some(row) = pending.take() { permit.send(row); }
                } else {
                    current = None;
                    pending = None;
                }
            }
            response = stream.message(), if pending.is_none() => {
                match response {
                    Ok(Some(response)) => {
                        if response.request_id < request_id { continue; }
                        if response.request_id != request_id {
                            if let Some(rows) = current.take() { let _ = rows.send(Err(ViewerClientError::Internal)).await; }
                            return;
                        }
                        match response.event {
                            Some(v1::stream_viewer_row_session_response::Event::Rows(row)) => {
                                if current.is_some() { pending = Some(Ok(row)); }
                            }
                            Some(v1::stream_viewer_row_session_response::Event::Completed(_)) => current = None,
                            Some(v1::stream_viewer_row_session_response::Event::Failed(error)) => {
                                if let Some(rows) = current.take() { let _ = rows.send(Err(decode_failure(error))).await; }
                            }
                            None => {
                                if let Some(rows) = current.take() { let _ = rows.send(Err(ViewerClientError::Internal)).await; }
                                return;
                            }
                        }
                    }
                    terminal => {
                        let error = terminal.err().as_ref().map_or(ViewerClientError::Unavailable, decode_status);
                        if let Some(rows) = current.take() { let _ = rows.send(Err(error)).await; }
                        return;
                    }
                }
            }
        }
    }
}

fn encode_demand(request_id: u64, request: StreamViewerRows) -> v1::StreamViewerRowSessionRequest {
    v1::StreamViewerRowSessionRequest {
        request_id,
        rows: Some(proto::viewer::encode_stream_viewer_rows_request(request)),
    }
}

fn decode_failure(failure: v1::ViewerRowSessionFailure) -> ViewerClientError {
    use v1::ViewerRowSessionFailureCode;
    match ViewerRowSessionFailureCode::try_from(failure.code) {
        Ok(ViewerRowSessionFailureCode::InvalidRequest) => ViewerClientError::InvalidRequest,
        Ok(ViewerRowSessionFailureCode::NotFound) => ViewerClientError::NotFound,
        Ok(ViewerRowSessionFailureCode::Conflict) => ViewerClientError::Conflict,
        Ok(ViewerRowSessionFailureCode::ResourceExhausted) => ViewerClientError::ResourceExhausted,
        Ok(ViewerRowSessionFailureCode::Unavailable) => ViewerClientError::Unavailable,
        Ok(ViewerRowSessionFailureCode::InvalidSettings) => ViewerClientError::InvalidSettings,
        Ok(ViewerRowSessionFailureCode::Internal | ViewerRowSessionFailureCode::Unspecified)
        | Err(_) => ViewerClientError::Internal,
    }
}
