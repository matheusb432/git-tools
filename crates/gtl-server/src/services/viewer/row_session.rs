use gtl_models::viewer::ViewerTabId;
use gtl_wire::v1::{self, stream_viewer_row_session_response::Event};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt as _, wrappers::ReceiverStream};
use tonic::Status;

use super::rows;
use crate::state::AppState;

pub(super) async fn start(
    state: AppState,
    mut requests: tonic::Streaming<v1::StreamViewerRowSessionRequest>,
) -> Result<ReceiverStream<Result<v1::StreamViewerRowSessionResponse, Status>>, Status> {
    let permit = state
        .viewer_row_sessions
        .clone()
        .try_acquire_owned()
        .map_err(|_| Status::resource_exhausted("viewer row session capacity is busy"))?;
    let first = tokio::time::timeout(std::time::Duration::from_secs(5), requests.message())
        .await
        .map_err(|_| Status::deadline_exceeded("viewer row session initialization timed out"))??
        .ok_or_else(|| Status::invalid_argument("row session requires initial demand"))?;
    let tab = request_tab(&first)?;
    let (sender, receiver) = mpsc::channel(2);
    tokio::spawn(async move {
        let _permit = permit;
        let result = run(&state, tab, first, &mut requests, &sender).await;
        if let Err(error) = result {
            let _ = sender.send(Err(error)).await;
        }
    });
    Ok(ReceiverStream::new(receiver))
}

fn request_tab(request: &v1::StreamViewerRowSessionRequest) -> Result<ViewerTabId, Status> {
    if request.request_id == 0 {
        return Err(Status::invalid_argument("row demand ID must be positive"));
    }
    let rows = request
        .rows
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("row demand is required"))?;
    if rows.row_range.is_none() || rows.file_id.is_none() {
        return Err(Status::invalid_argument(
            "row sessions require a bounded file range",
        ));
    }
    let identity = rows
        .identity
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("row identity is required"))?;
    ViewerTabId::try_new(identity.tab_id)
        .map_err(|_| Status::invalid_argument("row tab is invalid"))
}

async fn run(
    state: &AppState,
    tab: ViewerTabId,
    first: v1::StreamViewerRowSessionRequest,
    requests: &mut tonic::Streaming<v1::StreamViewerRowSessionRequest>,
    sender: &mpsc::Sender<Result<v1::StreamViewerRowSessionResponse, Status>>,
) -> Result<(), Status> {
    let mut versions = state.viewer.subscribe();
    let mut request_id = first.request_id;
    let (mut current, mut pending) = begin(state, first);
    loop {
        tokio::select! {
            biased;
            () = sender.closed() => return Ok(()),
            request = requests.message() => {
                let Some(request) = request? else { return Ok(()); };
                if request_tab(&request)? != tab || request.request_id <= request_id {
                    return Err(Status::invalid_argument("row demand must advance within its tab"));
                }
                request_id = request.request_id;
                // Release the superseded producer before admitting its replacement.
                drop(current.take());
                (current, pending) = begin(state, request);
            }
            changed = versions.changed() => {
                if changed.is_err() { return Ok(()); }
                if !state.viewer.inspect(|session| session.tab(tab).is_some())
                    .map_err(|_| Status::internal("viewer state is unavailable"))? {
                    return Ok(());
                }
            }
            permit = sender.reserve(), if pending.is_some() => {
                let permit = permit.map_err(|_| Status::cancelled("row session disconnected"))?;
                if let Some(event) = pending.take() {
                    permit.send(Ok(v1::StreamViewerRowSessionResponse { request_id, event: Some(event) }));
                }
            }
            item = async { match current.as_mut() { Some(stream) => stream.next().await, None => std::future::pending().await } }, if pending.is_none() => {
                pending = Some(match item {
                    Some(Ok(rows)) => Event::Rows(rows),
                    Some(Err(error)) => { current = None; failure(&error) }
                    None => { current = None; Event::Completed(v1::Empty {}) }
                });
            }
        }
    }
}

fn begin(
    state: &AppState,
    request: v1::StreamViewerRowSessionRequest,
) -> (Option<rows::RowStream>, Option<Event>) {
    match request
        .rows
        .ok_or_else(|| Status::invalid_argument("row demand is required"))
        .and_then(|request| rows::start(state.clone(), request))
    {
        Ok(stream) => (Some(stream), None),
        Err(error) => (None, Some(failure(&error))),
    }
}

fn failure(status: &Status) -> Event {
    use v1::ViewerRowSessionFailureCode;
    let code = if status
        .metadata()
        .get("gtl-error-kind")
        .is_some_and(|kind| kind == "invalid-user-settings")
    {
        ViewerRowSessionFailureCode::InvalidSettings
    } else {
        match status.code() {
            tonic::Code::InvalidArgument
            | tonic::Code::FailedPrecondition
            | tonic::Code::OutOfRange => ViewerRowSessionFailureCode::InvalidRequest,
            tonic::Code::NotFound => ViewerRowSessionFailureCode::NotFound,
            tonic::Code::Aborted | tonic::Code::AlreadyExists => {
                ViewerRowSessionFailureCode::Conflict
            }
            tonic::Code::ResourceExhausted => ViewerRowSessionFailureCode::ResourceExhausted,
            tonic::Code::Unavailable | tonic::Code::Cancelled | tonic::Code::DeadlineExceeded => {
                ViewerRowSessionFailureCode::Unavailable
            }
            _ => ViewerRowSessionFailureCode::Internal,
        }
    };
    Event::Failed(v1::ViewerRowSessionFailure { code: code.into() })
}
