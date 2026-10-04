use std::time::Duration;

use gtl_wire::{
    proto::terminal_diff::SnapshotDecoder,
    terminal_diff::{MESSAGE_BYTES_MAX, SnapshotError, TerminalDiff},
    v1,
};

use crate::{ClientError, GtlClient};

#[derive(Debug, thiserror::Error)]
pub enum TerminalDiffError {
    #[error("{0}")]
    Request(#[from] ClientError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
}

impl GtlClient {
    pub async fn set_diff_file_reviewed(
        &self,
        request: gtl_wire::diff_review::SetDiffFileReviewed,
    ) -> Result<(), ClientError> {
        let mut request = tonic::Request::new(gtl_wire::proto::diff_review::encode_set(&request));
        request.set_timeout(Duration::from_secs(5));
        let mut client = v1::viewer_service_client::ViewerServiceClient::new(self.channel.clone())
            .max_encoding_message_size(super::MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(super::MAX_RESPONSE_MESSAGE_SIZE);
        let operation = client.set_diff_file_reviewed(request);
        tokio::time::timeout(Duration::from_secs(10), operation)
            .await
            .map_err(|_| {
                ClientError::from(tonic::Status::deadline_exceeded(
                    "saving review progress timed out",
                ))
            })??;
        Ok(())
    }
    pub async fn read_terminal_diff(
        &self,
        request: v1::ReadTerminalDiffRequest,
    ) -> Result<TerminalDiff, TerminalDiffError> {
        let mut client =
            v1::terminal_diff_service_client::TerminalDiffServiceClient::new(self.channel.clone())
                .max_encoding_message_size(super::MAX_REQUEST_MESSAGE_SIZE)
                .max_decoding_message_size(MESSAGE_BYTES_MAX);
        let mut request = tonic::Request::new(request);
        request.set_timeout(Duration::from_secs(120));
        let operation = async {
            let mut stream = client
                .read_terminal_diff(request)
                .await
                .map_err(ClientError::from)?
                .into_inner();
            let mut decoder = SnapshotDecoder::default();
            while let Some(message) = stream.message().await.map_err(ClientError::from)? {
                decoder.accept(message)?;
            }
            Ok(decoder.finish()?)
        };
        tokio::time::timeout(Duration::from_secs(120), operation)
            .await
            .map_err(|_| {
                ClientError::from(tonic::Status::deadline_exceeded("terminal diff timed out"))
            })?
    }
}
