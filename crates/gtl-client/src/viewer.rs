#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
mod protobuf;

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
use std::time::Duration;

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
use gtl_wire::{
    v1::{self, viewer_service_client::ViewerServiceClient},
    viewer::{
        GetViewerHistoryCopy, ListViewerCommits, ListViewerHistory, OpenViewerDiffFile,
        OpenViewerHistory, SelectViewerCommit, SetViewerPreference, StreamViewerRows,
        VIEWER_ROW_MAX_ENCODED_BYTES, ViewerCommitPage, ViewerHistoryCopyPayload,
        ViewerHistoryPage, ViewerRowStreamItem, ViewerShell, ViewerStateChanged, ViewerTabRequest,
        ViewerUserSettings,
    },
};
#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
use tonic::{Request, metadata::MetadataValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ViewerClientError {
    #[error("This desktop viewer and gtl-server use different viewer protocol versions.")]
    ProtocolMismatch,
    #[error("The viewer rejected this request. Refresh the page and try again.")]
    InvalidRequest,
    #[error("This viewer item is no longer available.")]
    NotFound,
    #[error("The viewer changed while this action was running. Try again.")]
    Conflict,
    #[error("The desktop viewer is temporarily unavailable.")]
    Unavailable,
    #[error("The viewer could not complete this action.")]
    Internal,
}

impl ViewerClientError {
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::ProtocolMismatch => {
                "This desktop viewer and gtl-server use different versions. Update and restart both."
            }
            Self::InvalidRequest => {
                "The viewer rejected this request. Refresh the page and try again."
            }
            Self::NotFound => "This viewer item is no longer available.",
            Self::Conflict => "The viewer changed while this action was running. Try again.",
            Self::Unavailable => "The desktop viewer is temporarily unavailable.",
            Self::Internal => "The viewer could not complete this action.",
        }
    }
}

#[cfg(any(test, all(target_arch = "wasm32", feature = "viewer-web")))]
fn validate_viewer_protocol(protocol_version: u32) -> Result<(), ViewerClientError> {
    if protocol_version == gtl_wire::viewer::VIEWER_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ViewerClientError::ProtocolMismatch)
    }
}

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
const VIEWER_UNARY_TIMEOUT: Duration = Duration::from_mins(30);
#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
const VIEWER_RESPONSE_MAX_BYTES: usize = VIEWER_ROW_MAX_ENCODED_BYTES + 64 * 1024;

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
macro_rules! viewer_unary_methods {
    ($($name:ident($request:ty) -> $response:ty => $encode:ident, $rpc:ident, $decode:ident;)+) => {
        $(
            pub async fn $name(
                &mut self,
                request: $request,
            ) -> Result<$response, ViewerClientError> {
                let request = self.unary_request(protobuf::$encode(request));
                let response = self
                    .client
                    .$rpc(request)
                    .await
                    .map(tonic::Response::into_inner)
                    .map_err(decode_status)?;
                protobuf::$decode(response)
            }
        )+
    };
}

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
macro_rules! viewer_unary_methods_with_fallible_request {
    ($($name:ident($request:ty) -> $response:ty => $encode:ident, $rpc:ident, $decode:ident;)+) => {
        $(
            pub async fn $name(
                &mut self,
                request: $request,
            ) -> Result<$response, ViewerClientError> {
                let request = self.unary_request(protobuf::$encode(request)?);
                let response = self
                    .client
                    .$rpc(request)
                    .await
                    .map(tonic::Response::into_inner)
                    .map_err(decode_status)?;
                protobuf::$decode(response)
            }
        )+
    };
}

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
pub struct ViewerClient {
    client: ViewerServiceClient<tonic_web_wasm_client::Client>,
    authorization: MetadataValue<tonic::metadata::Ascii>,
}

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
impl ViewerClient {
    pub fn connect_grpc_web(
        endpoint: String,
        capability: &str,
        protocol_version: u32,
    ) -> Result<Self, ViewerClientError> {
        validate_viewer_protocol(protocol_version)?;
        let authorization = format!("Bearer {capability}")
            .parse()
            .map_err(|_| ViewerClientError::Unavailable)?;
        let client = ViewerServiceClient::new(tonic_web_wasm_client::Client::new(endpoint))
            .max_decoding_message_size(VIEWER_RESPONSE_MAX_BYTES);
        Ok(Self {
            client,
            authorization,
        })
    }

    pub async fn get_shell(&mut self) -> Result<ViewerShell, ViewerClientError> {
        let request = self.unary_request(v1::GetViewerShellRequest {});
        let response = self
            .client
            .get_viewer_shell(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(decode_status)?;
        protobuf::decode_get_viewer_shell_response(response)
    }

    viewer_unary_methods! {
        activate_tab(ViewerTabRequest) -> ViewerShell =>
            encode_activate_viewer_tab_request, activate_viewer_tab, decode_activate_viewer_tab_response;
        close_tab(ViewerTabRequest) -> ViewerShell =>
            encode_close_viewer_tab_request, close_viewer_tab, decode_close_viewer_tab_response;
        refresh_tab(ViewerTabRequest) -> ViewerShell =>
            encode_refresh_viewer_tab_request, refresh_viewer_tab, decode_refresh_viewer_tab_response;
        delete_live_tab(ViewerTabRequest) -> ViewerShell =>
            encode_delete_live_viewer_tab_request, delete_live_viewer_tab, decode_delete_live_viewer_tab_response;
        select_commit(SelectViewerCommit) -> ViewerShell =>
            encode_select_viewer_commit_request, select_viewer_commit, decode_select_viewer_commit_response;
        clear_commit_selection(ViewerTabRequest) -> ViewerShell =>
            encode_clear_viewer_commit_selection_request, clear_viewer_commit_selection, decode_clear_viewer_commit_selection_response;
        set_preference(SetViewerPreference) -> ViewerShell =>
            encode_set_viewer_preference_request, set_viewer_preference, decode_set_viewer_preference_response;
        list_commits(ListViewerCommits) -> ViewerCommitPage =>
            encode_list_viewer_commits_request, list_viewer_commits, decode_list_viewer_commits_response;
    }

    viewer_unary_methods_with_fallible_request! {
        list_history(ListViewerHistory) -> ViewerHistoryPage =>
            encode_list_viewer_history_request, list_viewer_history, decode_list_viewer_history_response;
        open_history(OpenViewerHistory) -> ViewerShell =>
            encode_open_viewer_history_request, open_viewer_history, decode_open_viewer_history_response;
    }

    pub async fn get_history_copy(
        &mut self,
        request: GetViewerHistoryCopy,
    ) -> Result<ViewerHistoryCopyPayload, ViewerClientError> {
        let request =
            self.unary_request(protobuf::encode_get_viewer_history_copy_request(request)?);
        let response = self
            .client
            .get_viewer_history_copy(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(decode_status)?;
        Ok(protobuf::decode_get_viewer_history_copy_response(response))
    }

    pub async fn get_settings(&mut self) -> Result<ViewerUserSettings, ViewerClientError> {
        let request = self.unary_request(v1::GetViewerSettingsRequest {});
        let response = self
            .client
            .get_viewer_settings(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(decode_status)?;
        protobuf::decode_get_viewer_settings_response(response)
    }

    pub async fn open_diff_file(
        &mut self,
        request: OpenViewerDiffFile,
    ) -> Result<(), ViewerClientError> {
        let request = self.unary_request(protobuf::encode_open_viewer_diff_file_request(request));
        self.client
            .open_viewer_diff_file(request)
            .await
            .map_err(decode_status)?;
        Ok(())
    }

    pub async fn stream_rows(
        &mut self,
        request: StreamViewerRows,
    ) -> Result<ViewerRowStream, ViewerClientError> {
        let request = self.streaming_request(protobuf::encode_stream_viewer_rows_request(request));
        let stream = self
            .client
            .stream_viewer_rows(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(decode_status)?;
        Ok(ViewerRowStream { stream })
    }

    pub async fn watch(&mut self) -> Result<ViewerVersionStream, ViewerClientError> {
        let request = self.streaming_request(v1::WatchViewerRequest {});
        let stream = self
            .client
            .watch_viewer(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(decode_status)?;
        Ok(ViewerVersionStream { stream })
    }

    fn unary_request<Message>(&self, message: Message) -> Request<Message> {
        let mut request = self.request(message);
        request.set_timeout(VIEWER_UNARY_TIMEOUT);
        request
    }

    fn streaming_request<Message>(&self, message: Message) -> Request<Message> {
        self.request(message)
    }

    fn request<Message>(&self, message: Message) -> Request<Message> {
        let mut request = Request::new(message);
        request
            .metadata_mut()
            .insert("authorization", self.authorization.clone());
        request
    }
}

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
pub struct ViewerRowStream {
    stream: tonic::Streaming<v1::StreamViewerRowsResponse>,
}

#[cfg(not(all(target_arch = "wasm32", feature = "viewer-web")))]
pub struct ViewerRowStream;

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
impl ViewerRowStream {
    pub async fn message(&mut self) -> Result<Option<ViewerRowStreamItem>, ViewerClientError> {
        self.stream
            .message()
            .await
            .map_err(decode_status)?
            .map(protobuf::decode_stream_viewer_rows_response)
            .transpose()
    }
}

#[cfg(not(all(target_arch = "wasm32", feature = "viewer-web")))]
impl ViewerRowStream {
    #[allow(clippy::unused_self)]
    pub fn message(
        &mut self,
    ) -> impl std::future::Future<
        Output = Result<Option<gtl_wire::viewer::ViewerRowStreamItem>, ViewerClientError>,
    > {
        std::future::ready(Err(ViewerClientError::Unavailable))
    }
}

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
pub struct ViewerVersionStream {
    stream: tonic::Streaming<v1::WatchViewerResponse>,
}

#[cfg(not(all(target_arch = "wasm32", feature = "viewer-web")))]
pub struct ViewerVersionStream;

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
impl ViewerVersionStream {
    pub async fn message(&mut self) -> Result<Option<ViewerStateChanged>, ViewerClientError> {
        self.stream
            .message()
            .await
            .map_err(decode_status)
            .map(|response| response.map(protobuf::decode_watch_viewer_response))
    }
}

#[cfg(not(all(target_arch = "wasm32", feature = "viewer-web")))]
impl ViewerVersionStream {
    #[allow(clippy::unused_self)]
    pub fn message(
        &mut self,
    ) -> impl std::future::Future<
        Output = Result<Option<gtl_wire::viewer::ViewerStateChanged>, ViewerClientError>,
    > {
        std::future::ready(Err(ViewerClientError::Unavailable))
    }
}

#[cfg(all(target_arch = "wasm32", feature = "viewer-web"))]
fn decode_status(status: tonic::Status) -> ViewerClientError {
    match status.code() {
        tonic::Code::InvalidArgument | tonic::Code::FailedPrecondition => {
            ViewerClientError::InvalidRequest
        }
        tonic::Code::NotFound => ViewerClientError::NotFound,
        tonic::Code::Aborted | tonic::Code::AlreadyExists => ViewerClientError::Conflict,
        tonic::Code::Unavailable
        | tonic::Code::DeadlineExceeded
        | tonic::Code::Cancelled
        | tonic::Code::Unauthenticated => ViewerClientError::Unavailable,
        _ => ViewerClientError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::{ViewerClientError, validate_viewer_protocol};

    #[test]
    fn viewer_protocol_mismatch_has_a_clear_client_error() {
        let current = gtl_wire::viewer::VIEWER_PROTOCOL_VERSION;
        let different = current
            .checked_add(1)
            .expect("protocol version can advance");

        assert_eq!(validate_viewer_protocol(current), Ok(()));
        assert_eq!(
            validate_viewer_protocol(different),
            Err(ViewerClientError::ProtocolMismatch)
        );
    }
}
