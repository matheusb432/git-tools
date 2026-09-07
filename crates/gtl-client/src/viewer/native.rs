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
        projects::{OpenViewerProject, OpenViewerProjectOk, ViewerProject},
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
        let stream = self
            .client
            .stream_viewer_rows(proto::viewer::encode_stream_viewer_rows_request(request))
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        Ok(ViewerRowStream { stream })
    }

    pub async fn watch(&mut self) -> Result<ViewerVersionStream, ViewerClientError> {
        let stream = self
            .client
            .watch_viewer(v1::WatchViewerRequest {})
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        Ok(ViewerVersionStream { stream })
    }
}

pub struct ViewerRowStream {
    stream: tonic::Streaming<v1::StreamViewerRowsResponse>,
}

impl ViewerRowStream {
    pub async fn message(&mut self) -> Result<Option<ViewerRowStreamItem>, ViewerClientError> {
        self.stream
            .message()
            .await
            .map_err(|status| decode_status(&status))?
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
            .map_err(|status| decode_status(&status))
            .map(|response| response.map(proto::viewer::decode_watch_viewer_response))
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
