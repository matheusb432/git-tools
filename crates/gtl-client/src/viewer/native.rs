mod row_sessions;

use gtl_local_auth::LocalAuth;
use gtl_wire::{
    proto,
    v1::{self, viewer_service_client::ViewerServiceClient},
    viewer::{
        EditSettingsRequest, FindViewerDiff, GetViewerHistoryCopy, ListViewerCommits,
        ListViewerHistory, MoveViewerTab, OpenViewerDiffFile, OpenViewerHistory, SearchViewerFiles,
        SelectViewerCommit, SetViewerPreference, StreamViewerRows, VIEWER_ROW_MAX_ENCODED_BYTES,
        ViewerCommitPage, ViewerDiffSearchResult, ViewerFileSearchResult, ViewerHistoryCopyPayload,
        ViewerHistoryPage, ViewerRowStreamItem, ViewerShell, ViewerStateChanged, ViewerTabRequest,
        ViewerUserSettings,
        projects::{OpenViewerProject, OpenViewerProjectOk, UpdateViewerProject, ViewerProject},
    },
};

use super::{ViewerClientError, validate_viewer_protocol};
use crate::{AuthenticatedChannel, GtlClient};

const VIEWER_RESPONSE_MAX_BYTES: usize = VIEWER_ROW_MAX_ENCODED_BYTES + 64 * 1024;

macro_rules! viewer_unary_methods {
    ($($name:ident($request:ty) -> $response:ty => $encode:ident, $rpc:ident, $decode:ident;)+) => {
        $(
            pub async fn $name(
                &mut self,
                request: $request,
            ) -> Result<$response, ViewerClientError> {
                let response = self
                    .client
                    .$rpc(proto::viewer::$encode(request))
                    .await
                    .map(tonic::Response::into_inner)
                    .map_err(|status| decode_status(&status))?;
                proto::viewer::$decode(response).map_err(Into::into)
            }
        )+
    };
}

macro_rules! viewer_unary_methods_with_fallible_request {
    ($($name:ident($request:ty) -> $response:ty => $encode:ident, $rpc:ident, $decode:ident;)+) => {
        $(
            pub async fn $name(
                &mut self,
                request: $request,
            ) -> Result<$response, ViewerClientError> {
                let request = proto::viewer::$encode(request)?;
                let response = self
                    .client
                    .$rpc(request)
                    .await
                    .map(tonic::Response::into_inner)
                    .map_err(|status| decode_status(&status))?;
                proto::viewer::$decode(response).map_err(Into::into)
            }
        )+
    };
}

#[derive(Clone)]
pub struct ViewerClient {
    client: ViewerServiceClient<AuthenticatedChannel>,
    server_instance_id: String,
    protocol_version: u32,
    row_sessions: std::sync::Arc<tokio::sync::Mutex<row_sessions::RowSessions>>,
}

impl ViewerClient {
    pub async fn list_projects(&mut self) -> Result<Vec<ViewerProject>, ViewerClientError> {
        let response = self
            .client
            .list_viewer_projects(v1::ListViewerProjectsRequest {})
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| {
                if status.code() == tonic::Code::FailedPrecondition {
                    ViewerClientError::ProjectsUnavailable
                } else {
                    decode_status(&status)
                }
            })?;
        proto::viewer::projects::decode_projects(response).map_err(Into::into)
    }

    pub async fn update_project(
        &mut self,
        request: UpdateViewerProject,
    ) -> Result<(), ViewerClientError> {
        self.client
            .update_viewer_project(proto::viewer::projects::encode_update(request))
            .await
            .map_err(|status| decode_status(&status))?;
        Ok(())
    }

    pub async fn open_project(
        &mut self,
        request: OpenViewerProject,
    ) -> Result<OpenViewerProjectOk, ViewerClientError> {
        let response = self
            .client
            .open_viewer_project(proto::viewer::projects::encode_open(&request))
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        proto::viewer::projects::decode_open_response(response).map_err(Into::into)
    }

    /// Discovers the local server and connects through its authenticated native endpoint.
    pub async fn connect_local() -> Result<Self, ViewerClientError> {
        let auth = LocalAuth::from_environment().map_err(|_| ViewerClientError::Unavailable)?;
        Self::connect(&auth).await
    }

    /// Connects to the server published in `auth`'s data root.
    pub async fn connect(auth: &LocalAuth) -> Result<Self, ViewerClientError> {
        let bootstrap = auth
            .load_viewer_bootstrap()
            .map_err(|_| ViewerClientError::Unavailable)?;
        validate_viewer_protocol(bootstrap.protocol_version())?;
        let native = GtlClient::connect(auth)
            .await
            .map_err(|_| ViewerClientError::Unavailable)?;
        if native.endpoint.instance_id() != bootstrap.instance_id() {
            return Err(ViewerClientError::Unavailable);
        }
        let client = ViewerServiceClient::with_interceptor(native.channel, native.request_policy)
            .max_decoding_message_size(VIEWER_RESPONSE_MAX_BYTES);
        Ok(Self {
            client,
            server_instance_id: bootstrap.instance_id().to_string(),
            protocol_version: bootstrap.protocol_version(),
            row_sessions: std::sync::Arc::default(),
        })
    }

    #[must_use]
    pub fn server_instance_id(&self) -> &str {
        &self.server_instance_id
    }

    #[must_use]
    pub const fn protocol_version(&self) -> u32 {
        self.protocol_version
    }

    pub async fn get_shell(&mut self) -> Result<ViewerShell, ViewerClientError> {
        let response = self
            .client
            .get_viewer_shell(v1::GetViewerShellRequest {})
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        proto::viewer::decode_get_viewer_shell_response(response).map_err(Into::into)
    }

    viewer_unary_methods! {
        activate_tab(ViewerTabRequest) -> ViewerShell =>
            encode_activate_viewer_tab_request, activate_viewer_tab, decode_activate_viewer_tab_response;
        move_tab(MoveViewerTab) -> ViewerShell =>
            encode_move_viewer_tab_request, move_viewer_tab, decode_move_viewer_tab_response;
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
        search_files(SearchViewerFiles) -> ViewerFileSearchResult =>
            encode_search_viewer_files_request, search_viewer_files, decode_search_viewer_files_response;
        find_diff(FindViewerDiff) -> ViewerDiffSearchResult =>
            encode_find_viewer_diff_request, find_viewer_diff, decode_find_viewer_diff_response;
    }

    viewer_unary_methods_with_fallible_request! {
        list_history(ListViewerHistory) -> ViewerHistoryPage =>
            encode_list_viewer_history_request, list_viewer_history, decode_list_viewer_history_response;
        open_history(OpenViewerHistory) -> ViewerShell =>
            encode_open_viewer_history_request, open_viewer_history, decode_open_viewer_history_response;
    }

    pub async fn read_diff_text(
        &mut self,
        request: gtl_wire::viewer::ReadViewerDiffText,
    ) -> Result<Vec<gtl_wire::viewer::ViewerDiffTextLine>, ViewerClientError> {
        let response = self
            .client
            .read_viewer_diff_text(proto::viewer::text::encode_request(&request))
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        proto::viewer::text::decode_response(response).map_err(Into::into)
    }

    pub async fn get_history_copy(
        &mut self,
        request: GetViewerHistoryCopy,
    ) -> Result<ViewerHistoryCopyPayload, ViewerClientError> {
        let response = self
            .client
            .get_viewer_history_copy(proto::viewer::encode_get_viewer_history_copy_request(
                request,
            )?)
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        Ok(proto::viewer::decode_get_viewer_history_copy_response(
            response,
        ))
    }

    pub async fn get_settings_recovery(
        &mut self,
    ) -> Result<gtl_wire::viewer::ViewerSettingsRecovery, ViewerClientError> {
        let response = self
            .client
            .get_settings_recovery(v1::GetSettingsRecoveryRequest {})
            .await
            .map_err(|status| decode_status(&status))?
            .into_inner();
        Ok(proto::viewer::decode_get_settings_recovery_response(
            response,
        ))
    }

    pub async fn reset_settings(
        &mut self,
        request: gtl_wire::viewer::ResetSettings,
    ) -> Result<gtl_wire::viewer::ResetSettingsOk, ViewerClientError> {
        let response = self
            .client
            .reset_settings(proto::viewer::encode_reset_settings_request(request))
            .await
            .map_err(|status| decode_status(&status))?
            .into_inner();
        Ok(proto::viewer::decode_reset_settings_response(response))
    }

    pub async fn get_settings(&mut self) -> Result<ViewerUserSettings, ViewerClientError> {
        let response = self
            .client
            .get_viewer_settings(v1::GetViewerSettingsRequest {})
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        proto::viewer::decode_get_viewer_settings_response(response).map_err(Into::into)
    }

    pub async fn edit_settings(
        &mut self,
        request: EditSettingsRequest,
    ) -> Result<(), ViewerClientError> {
        self.client
            .edit_settings(proto::viewer::encode_edit_settings_request(request))
            .await
            .map_err(|status| decode_status(&status))?;
        Ok(())
    }

    pub async fn open_diff_file(
        &mut self,
        request: OpenViewerDiffFile,
    ) -> Result<(), ViewerClientError> {
        self.client
            .open_viewer_diff_file(proto::viewer::encode_open_viewer_diff_file_request(request))
            .await
            .map_err(|status| decode_status(&status))?;
        Ok(())
    }

    pub async fn stream_rows(
        &mut self,
        request: StreamViewerRows,
    ) -> Result<ViewerRowStream, ViewerClientError> {
        if request.row_range.is_none() {
            let stream = self
                .client
                .stream_viewer_rows(proto::viewer::encode_stream_viewer_rows_request(request))
                .await
                .map_err(|status| decode_status(&status))?
                .into_inner();
            return Ok(ViewerRowStream {
                stream: RowResponseStream::Complete(Box::new(stream)),
            });
        }
        self.row_sessions
            .lock()
            .await
            .request(self.client.clone(), request)
    }

    pub async fn watch(
        &mut self,
        request: gtl_wire::viewer::WatchViewer,
    ) -> Result<ViewerVersionStream, ViewerClientError> {
        let stream = self
            .client
            .watch_viewer(v1::WatchViewerRequest {
                live_tab_id: request.live_tab_id.map(Into::into),
            })
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        Ok(ViewerVersionStream { stream })
    }
}

enum RowResponseStream {
    Window(tokio::sync::mpsc::Receiver<Result<v1::StreamViewerRowsResponse, ViewerClientError>>),
    Complete(Box<tonic::Streaming<v1::StreamViewerRowsResponse>>),
}

pub struct ViewerRowStream {
    stream: RowResponseStream,
}

impl ViewerRowStream {
    async fn next(&mut self) -> Result<Option<v1::StreamViewerRowsResponse>, ViewerClientError> {
        match &mut self.stream {
            RowResponseStream::Window(stream) => stream.recv().await.transpose(),
            RowResponseStream::Complete(stream) => stream
                .message()
                .await
                .map_err(|status| decode_status(&status)),
        }
    }

    pub async fn message_bytes(&mut self) -> Result<Option<Vec<u8>>, ViewerClientError> {
        self.next()
            .await?
            .as_ref()
            .map(proto::row_ipc::encode_frame)
            .transpose()
            .map_err(Into::into)
    }

    pub async fn message(&mut self) -> Result<Option<ViewerRowStreamItem>, ViewerClientError> {
        self.next()
            .await?
            .map(proto::viewer::decode_stream_viewer_rows_response)
            .transpose()
            .map_err(Into::into)
    }
}

pub struct ViewerVersionStream {
    stream: tonic::Streaming<v1::WatchViewerResponse>,
}

impl ViewerVersionStream {
    pub async fn message(&mut self) -> Result<Option<ViewerStateChanged>, ViewerClientError> {
        self.stream
            .message()
            .await
            .map_err(|status| decode_status(&status))?
            .map(proto::viewer::decode_watch_viewer_response)
            .transpose()
            .map_err(Into::into)
    }
}

impl From<proto::viewer::ViewerCodecError> for ViewerClientError {
    fn from(error: proto::viewer::ViewerCodecError) -> Self {
        match error {
            proto::viewer::ViewerCodecError::Unrepresentable => Self::InvalidRequest,
            proto::viewer::ViewerCodecError::InvalidMessage => Self::Internal,
        }
    }
}

fn decode_status(status: &tonic::Status) -> ViewerClientError {
    if status.code() == tonic::Code::FailedPrecondition
        && status
            .metadata()
            .get("gtl-error-kind")
            .is_some_and(|kind| kind == "invalid-user-settings")
    {
        return ViewerClientError::InvalidSettings;
    }
    match status.code() {
        tonic::Code::InvalidArgument | tonic::Code::FailedPrecondition => {
            ViewerClientError::InvalidRequest
        }
        tonic::Code::NotFound => ViewerClientError::NotFound,
        tonic::Code::Aborted | tonic::Code::AlreadyExists => ViewerClientError::Conflict,
        tonic::Code::ResourceExhausted => ViewerClientError::ResourceExhausted,
        tonic::Code::Unavailable
        | tonic::Code::DeadlineExceeded
        | tonic::Code::Cancelled
        | tonic::Code::Unauthenticated => ViewerClientError::Unavailable,
        _ => ViewerClientError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_settings_uses_structured_metadata_instead_of_message_text() {
        let mut status = tonic::Status::failed_precondition("any diagnostic");
        assert_eq!(decode_status(&status), ViewerClientError::InvalidRequest);
        status
            .metadata_mut()
            .insert("gtl-error-kind", "invalid-user-settings".parse().unwrap());
        assert_eq!(decode_status(&status), ViewerClientError::InvalidSettings);
        assert_eq!(
            decode_status(&tonic::Status::failed_precondition(
                "user settings are invalid"
            )),
            ViewerClientError::InvalidRequest
        );
    }
}
