use gtl_models::{
    failure::{ErrorClass, Failure},
    viewer::ViewerTabId,
};
use gtl_wire::v1::{self, stream_viewer_row_session_response::Event};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt as _, wrappers::ReceiverStream};
use tonic::Status;

use super::{
    super::status::{invalid_request, private, status},
    rows,
};
use crate::state::AppState;

pub(super) async fn start(
    state: AppState,
    mut requests: tonic::Streaming<v1::StreamViewerRowSessionRequest>,
) -> Result<ReceiverStream<Result<v1::StreamViewerRowSessionResponse, Status>>, Status> {
    let permit = state
        .viewer_row_sessions
        .clone()
        .try_acquire_owned()
        .map_err(|_| status(&Failure::Busy))?;
    let first = tokio::time::timeout(std::time::Duration::from_secs(5), requests.message())
        .await
        .map_err(|_| {
            private(
                ErrorClass::DeadlineExceeded,
                "viewer row session initialization timed out",
            )
        })??
        .ok_or_else(|| invalid_request("rows"))?;
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
        return Err(invalid_request("request_id"));
    }
    let rows = request
        .rows
        .as_ref()
        .ok_or_else(|| invalid_request("rows"))?;
    if rows.row_range.is_none() {
        return Err(invalid_request("rows.row_range"));
    }
    if rows.file_id.is_none() {
        return Err(invalid_request("rows.file_id"));
    }
    let identity = rows
        .identity
        .as_ref()
        .ok_or_else(|| invalid_request("rows.identity"))?;
    ViewerTabId::try_new(identity.tab_id).map_err(|_| invalid_request("rows.identity.tab_id"))
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
                    return Err(invalid_request("request_id"));
                }
                request_id = request.request_id;
                // Release the superseded producer before admitting its replacement.
                drop(current.take());
                (current, pending) = begin(state, request);
            }
            changed = versions.changed() => {
                if changed.is_err() { return Ok(()); }
                if !state.viewer.inspect(|session| session.tab(tab).is_some())
                    .map_err(|_| private(ErrorClass::Internal, "viewer state is unavailable"))? {
                    return Ok(());
                }
            }
            permit = sender.reserve(), if pending.is_some() => {
                let permit = permit.map_err(|_| private(ErrorClass::Cancelled, "row session disconnected"))?;
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
        .ok_or_else(|| invalid_request("rows"))
        .and_then(|request| rows::start(state.clone(), request))
    {
        Ok(stream) => (Some(stream), None),
        Err(error) => (None, Some(failure(&error))),
    }
}

fn failure(status: &Status) -> Event {
    use gtl_wire::proto::failure::{StatusFailure, code_class, decode_status, encode_failure};
    let failure = match decode_status(status) {
        StatusFailure::Decoded(failure) => failure,
        StatusFailure::Unrecognized | StatusFailure::Absent => {
            Failure::private(code_class(status.code()))
        }
    };
    Event::Failed(v1::ViewerRowSessionFailure {
        failure: Some(encode_failure(&failure)),
    })
}
