use std::{sync::Arc, time::Duration};

use gtl_application::diffs::read_terminal_diff::{self, ReadTerminalDiff};
use gtl_models::failure::{Failure, ViewerFailure};
use gtl_wire::{
    proto::terminal_diff::encode_row,
    terminal_diff::{BATCH_ROWS_MAX, MESSAGE_BYTES_MAX, Row, TerminalDiff},
    v1::{
        self, read_terminal_diff_response::Event, terminal_diff_service_server::TerminalDiffService,
    },
};
use prost::Message as _;
use tokio::sync::{Semaphore, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use super::{
    status::{GrpcResultExt as _, status},
    unexpected,
};
use crate::state::AppState;

pub(crate) struct TerminalDiffGrpcService {
    state: AppState,
    workers: Arc<Semaphore>,
}

impl TerminalDiffGrpcService {
    pub(crate) fn new(state: AppState) -> Self {
        Self {
            state,
            workers: Arc::new(Semaphore::new(2)),
        }
    }
}

#[tonic::async_trait]
impl TerminalDiffService for TerminalDiffGrpcService {
    type ReadTerminalDiffStream = ReceiverStream<Result<v1::ReadTerminalDiffResponse, Status>>;

    async fn read_terminal_diff(
        &self,
        request: Request<v1::ReadTerminalDiffRequest>,
    ) -> Result<Response<Self::ReadTerminalDiffStream>, Status> {
        let request = request.into_inner();
        let request = ReadTerminalDiff {
            cwd: super::absolute_path(request.working_directory, "working_directory")?,
            target: super::diff::validated_diff_target(request.target)?,
        };
        let permit = self
            .workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| status(&Failure::Busy))?;
        let state = self.state.clone();
        let (sender, receiver) = mpsc::channel(2);
        tokio::spawn(async move {
            let operation = async {
                let (snapshot, _permit) = tokio::task::spawn_blocking(move || {
                    read_terminal_diff::execute(&request, &state.git, &state.database)
                        .into_grpc()
                        .map(|snapshot| (snapshot, permit))
                })
                .await
                .map_err(|error| unexpected(error, "read terminal diff"))??;
                send_snapshot(&sender, &snapshot).await
            };
            tokio::select! {
                () = sender.closed() => {},
                result = tokio::time::timeout(Duration::from_secs(120), operation) => {
                    let result = result.unwrap_or_else(|_| Err(status(&Failure::Unavailable)));
                    if let Err(error) = result { let _ = tokio::time::timeout(Duration::from_secs(1), sender.send(Err(error))).await; }
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(receiver)))
    }
}

async fn send(
    sender: &mpsc::Sender<Result<v1::ReadTerminalDiffResponse, Status>>,
    event: Event,
) -> Result<(), Status> {
    let response = v1::ReadTerminalDiffResponse { event: Some(event) };
    if response.encoded_len() > MESSAGE_BYTES_MAX {
        return Err(status(&ViewerFailure::ResponseTooLarge));
    }
    sender
        .send(Ok(response))
        .await
        .map_err(|_| status(&Failure::Unavailable))
}

async fn send_snapshot(
    sender: &mpsc::Sender<Result<v1::ReadTerminalDiffResponse, Status>>,
    snapshot: &TerminalDiff,
) -> Result<(), Status> {
    send(
        sender,
        Event::Header(v1::TerminalDiffHeader {
            title: snapshot.title().to_owned(),
            notes: snapshot.notes().to_vec(),
        }),
    )
    .await?;
    for file in snapshot.files() {
        send(
            sender,
            Event::File(v1::TerminalDiffFile {
                path: file.path.clone(),
                added: file.added,
                removed: file.removed,
            }),
        )
        .await?;
        send_rows(sender, &file.compact, false).await?;
        send_rows(sender, &file.full, true).await?;
    }
    send(sender, Event::Completed(v1::Empty {})).await
}

async fn send_rows(
    sender: &mpsc::Sender<Result<v1::ReadTerminalDiffResponse, Status>>,
    rows: &[Row],
    full_context: bool,
) -> Result<(), Status> {
    let mut batch = v1::TerminalDiffRows {
        full_context,
        rows: Vec::new(),
    };
    let mut bytes = 32;
    for row in rows {
        let row = encode_row(row).map_err(|error| unexpected(error, "encode terminal row"))?;
        let size = row.encoded_len() + 8;
        if !batch.rows.is_empty()
            && (batch.rows.len() == BATCH_ROWS_MAX || bytes + size > MESSAGE_BYTES_MAX)
        {
            send(
                sender,
                Event::Rows(std::mem::replace(
                    &mut batch,
                    v1::TerminalDiffRows {
                        full_context,
                        rows: Vec::new(),
                    },
                )),
            )
            .await?;
            bytes = 32;
        }
        bytes += size;
        batch.rows.push(row);
    }
    if !batch.rows.is_empty() {
        send(sender, Event::Rows(batch)).await?;
    }
    Ok(())
}
