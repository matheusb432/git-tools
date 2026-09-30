use gtl_wire::viewer::{
    SetViewerChangesSince, SetViewerModifiedFiles, SetViewerTabLive, SetViewerTabPinned,
};
mod row_sessions;

use gtl_local_transport::LocalEndpoint;
use gtl_wire::{
    proto,
    v1::{
        self, project_service_client::ProjectServiceClient,
        viewer_service_client::ViewerServiceClient,
    },
    viewer::{
        EditSettingsRequest, FindViewerDiff, GetViewerHistoryCopy, ListViewerCommits,
        ListViewerHistory, MoveViewerTab, OpenViewerDiffFile, OpenViewerHistory, SearchViewerFiles,
        SelectViewerCommit, SetViewerPreference, StreamViewerRows, VIEWER_ROW_MAX_ENCODED_BYTES,
        ViewerCommitPage, ViewerDiffSearchResult, ViewerFileSearchResult, ViewerHistoryCopyPayload,
        ViewerHistoryPage, ViewerRowStreamItem, ViewerShell, ViewerStateChanged, ViewerTabRequest,
        ViewerUserSettings,
        projects::{
            DiscoverProjectRepositories, GetViewerProjectStatus, ImportProjectRepositories,
            ListViewerProjects, OpenUnpushedProjectDiffsOk, OpenViewerProject, OpenViewerProjectOk,
            ProjectDiscovery, ProjectImportResult, SetViewerProjectStatus, UpdateViewerProject,
            ViewerProjectPage, ViewerProjectStatus,
        },
    },
};

use super::{ViewerClientError, validate_viewer_protocol};
use crate::{GtlClient, RequestFailure};

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
    client: ViewerServiceClient<tonic::transport::Channel>,
    projects: ProjectServiceClient<tonic::transport::Channel>,
    server_instance_id: String,
    protocol_version: u32,
    row_sessions: std::sync::Arc<tokio::sync::Mutex<row_sessions::RowSessions>>,
}

impl ViewerClient {
    pub async fn get_push_availability(
        &mut self,
        identity: gtl_wire::viewer::ViewerViewIdentity,
    ) -> Result<gtl_wire::viewer::push::ViewerPushState, ViewerClientError> {
        let response = self
            .client
            .get_viewer_push_availability(proto::viewer::push::encode_availability_request(
                identity,
            ))
            .await
            .map_err(|error| decode_status(&error))?
            .into_inner();
        proto::viewer::push::decode_availability(response).map_err(Into::into)
    }

    pub async fn create_push(
        &mut self,
        request: gtl_wire::viewer::push::CreateViewerPush,
    ) -> Result<gtl_wire::viewer::push::ViewerPushRequest, ViewerClientError> {
        let response = self
            .client
            .create_viewer_push(proto::viewer::push::encode_create(request))
            .await
            .map_err(|error| decode_status(&error))?
            .into_inner();
        Ok(gtl_wire::viewer::push::ViewerPushRequest {
            id: proto::viewer::push::decode_id(&response.id)?,
        })
    }
    pub async fn get_push(
        &mut self,
        request: gtl_wire::viewer::push::ViewerPushRequest,
    ) -> Result<gtl_wire::viewer::push::ViewerPushStatus, ViewerClientError> {
        let response = self
            .client
            .get_viewer_push(v1::GetViewerPushRequest {
                id: request.id.to_string(),
            })
            .await
            .map_err(|error| decode_status(&error))?
            .into_inner();
        proto::viewer::push::decode_status(response).map_err(Into::into)
    }
    pub async fn start_push(
        &mut self,
        request: gtl_wire::viewer::push::ViewerPushRequest,
    ) -> Result<(), ViewerClientError> {
        self.client
            .start_viewer_push(v1::StartViewerPushRequest {
                id: request.id.to_string(),
            })
            .await
            .map_err(|error| decode_status(&error))?;
        Ok(())
    }

    pub async fn get_file_filters(
        &mut self,
        request: gtl_wire::viewer::ViewerTabRequest,
    ) -> Result<gtl_wire::viewer::file_filters::ViewerFileFilters, ViewerClientError> {
        let response = self
            .client
            .get_viewer_file_filters(proto::viewer::file_filters::encode_get(request.tab_id))
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        proto::viewer::file_filters::decode_filters(response).map_err(Into::into)
    }

    pub async fn set_file_filters(
        &mut self,
        request: gtl_wire::viewer::file_filters::SetViewerFileFilters,
    ) -> Result<(), ViewerClientError> {
        let response = self
            .client
            .set_viewer_file_filters(proto::viewer::file_filters::encode_set(request))
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        let _ = response;
        Ok(())
    }

    pub async fn close_other_tabs(
        &mut self,
        request: ViewerTabRequest,
    ) -> Result<(), ViewerClientError> {
        self.client
            .close_other_viewer_tabs(proto::viewer::encode_close_other_viewer_tabs_request(
                request,
            ))
            .await
            .map_err(|status| decode_status(&status))?;
        Ok(())
    }

    pub async fn rename_snapshot(
        &mut self,
        request: gtl_wire::viewer::RenameViewerSnapshot,
    ) -> Result<(), ViewerClientError> {
        self.client
            .rename_viewer_snapshot(proto::viewer::encode_rename_viewer_snapshot_request(
                request,
            ))
            .await
            .map_err(|status| decode_status(&status))?;
        Ok(())
    }

    pub async fn set_tab_pinned(
        &mut self,
        request: SetViewerTabPinned,
    ) -> Result<(), ViewerClientError> {
        self.client
            .set_viewer_tab_pinned(proto::viewer::encode_set_viewer_tab_pinned_request(request))
            .await
            .map_err(|status| decode_status(&status))?;
        Ok(())
    }

    pub async fn set_modified_files(
        &mut self,
        request: SetViewerModifiedFiles,
    ) -> Result<(), ViewerClientError> {
        self.client
            .set_viewer_modified_files(proto::viewer::encode_set_viewer_modified_files_request(
                request,
            ))
            .await
            .map_err(|status| decode_status(&status))?;
        Ok(())
    }

    pub async fn list_projects(
        &mut self,
        request: ListViewerProjects,
    ) -> Result<ViewerProjectPage, ViewerClientError> {
        let response = self
            .client
            .list_viewer_projects(proto::viewer::projects::encode_list(request))
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        proto::viewer::projects::decode_projects(response).map_err(Into::into)
    }

    pub async fn discover_project_repositories(
        &mut self,
        request: DiscoverProjectRepositories,
    ) -> Result<ProjectDiscovery, ViewerClientError> {
        let response = self
            .client
            .discover_project_repositories(proto::viewer::projects::encode_discover(request))
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        proto::viewer::projects::decode_discovery(response).map_err(Into::into)
    }

    pub async fn import_project_repositories(
        &mut self,
        request: ImportProjectRepositories,
    ) -> Result<Vec<ProjectImportResult>, ViewerClientError> {
        let response = self
            .client
            .import_project_repositories(proto::viewer::projects::encode_import(request))
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        proto::viewer::projects::decode_import_results(response).map_err(Into::into)
    }

    pub async fn get_project_status(
        &mut self,
        request: GetViewerProjectStatus,
    ) -> Result<ViewerProjectStatus, ViewerClientError> {
        let response = self
            .client
            .get_viewer_project_status(proto::viewer::projects::encode_get_status(request))
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        proto::viewer::projects::decode_project_status(response).map_err(Into::into)
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

    pub async fn set_project_status(
        &mut self,
        request: SetViewerProjectStatus,
    ) -> Result<(), ViewerClientError> {
        use gtl_models::projects::catalogue::ProjectStatus;
        let project_id = request.project_id.into_inner();
        let mode = v1::ProjectOperationMode::Apply.into();
        match request.status {
            ProjectStatus::Paused => self
                .projects
                .pause_project(v1::PauseProjectRequest { project_id, mode })
                .await
                .map(drop),
            ProjectStatus::Active => self
                .projects
                .resume_project(v1::ResumeProjectRequest { project_id, mode })
                .await
                .map(drop),
        }
        .map_err(|status| decode_status(&status))
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

    pub async fn open_unpushed_project_diffs(
        &mut self,
    ) -> Result<OpenUnpushedProjectDiffsOk, ViewerClientError> {
        let response = self
            .client
            .open_unpushed_project_diffs(v1::OpenUnpushedProjectDiffsRequest {})
            .await
            .map(tonic::Response::into_inner)
            .map_err(|status| decode_status(&status))?;
        Ok(proto::viewer::projects::decode_open_unpushed_response(
            response,
        ))
    }

    /// Resolves the local server and connects through its private native endpoint.
    pub async fn connect_local() -> Result<Self, ViewerClientError> {
        let endpoint =
            LocalEndpoint::from_environment().map_err(|_| ViewerClientError::Disconnected)?;
        Self::connect(&endpoint).await
    }

    /// Connects to an explicitly resolved local endpoint.
    pub async fn connect(endpoint: &LocalEndpoint) -> Result<Self, ViewerClientError> {
        let native = GtlClient::connect(endpoint)
            .await
            .map_err(|_| ViewerClientError::Disconnected)?;
        let server_info = native
            .get_viewer_server_info()
            .await
            .map_err(|_| ViewerClientError::Disconnected)?;
        validate_viewer_protocol(server_info.protocol_version())?;
        let projects = ProjectServiceClient::new(native.channel.clone());
        let client = ViewerServiceClient::new(native.channel)
            .max_decoding_message_size(VIEWER_RESPONSE_MAX_BYTES);
        Ok(Self {
            client,
            projects,
            server_instance_id: server_info.server_instance_id().to_owned(),
            protocol_version: server_info.protocol_version(),
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
        update_tab(ViewerTabRequest) -> ViewerShell =>
            encode_update_viewer_tab_request, update_viewer_tab, decode_update_viewer_tab_response;
        set_tab_live(SetViewerTabLive) -> ViewerShell =>
            encode_set_viewer_tab_live_request, set_viewer_tab_live, decode_set_viewer_tab_live_response;
        set_changes_since(SetViewerChangesSince) -> ViewerShell =>
            encode_set_viewer_changes_since_request, set_viewer_changes_since, decode_set_viewer_changes_since_response;
        select_commit(SelectViewerCommit) -> ViewerShell =>
            encode_select_viewer_commit_request, select_viewer_commit, decode_select_viewer_commit_response;
        clear_commit_selection(ViewerTabRequest) -> ViewerShell =>
            encode_clear_viewer_commit_selection_request, clear_viewer_commit_selection, decode_clear_viewer_commit_selection_response;
        set_preference(SetViewerPreference) -> ViewerShell =>
            encode_set_viewer_preference_request, set_viewer_preference, decode_set_viewer_preference_response;
        search_commits(gtl_wire::viewer::commit_search::SearchViewerCommits) -> gtl_wire::viewer::commit_search::ViewerCommitSearchResult =>
            encode_search_viewer_commits_request, search_viewer_commits, decode_search_viewer_commits_response;
        open_commit(gtl_wire::viewer::commit_search::OpenViewerCommit) -> OpenViewerProjectOk =>
            encode_open_viewer_commit_request, open_viewer_commit, decode_open_viewer_commit_response;
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
            .edit_settings(proto::viewer::encode_edit_settings_request(&request))
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
                project_ids: request
                    .projects
                    .ids()
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
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
    fn from(_: proto::viewer::ViewerCodecError) -> Self {
        Self::InvalidMessage
    }
}

/// Decodes the typed failure a gtl server attached to `status`.
fn decode_status(status: &tonic::Status) -> ViewerClientError {
    match RequestFailure::from_status(status) {
        RequestFailure::Failed(failure) => ViewerClientError::Failed(failure),
        RequestFailure::Disconnected => ViewerClientError::Disconnected,
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::failure::{Failure, PushFailure};

    use super::*;

    #[test]
    fn statuses_become_viewer_failures_or_disconnection() {
        let failure = Failure::Push(PushFailure::NothingToPush);
        let status = proto::failure::encode_status(failure.class(), &failure);

        assert_eq!(decode_status(&status), ViewerClientError::Failed(failure));
        assert_eq!(
            decode_status(&tonic::Status::unavailable("connection refused")),
            ViewerClientError::Disconnected
        );
    }
}
