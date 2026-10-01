mod cancellation;
mod commits;
mod file_filters;
mod files;
mod history;
mod live_watch;
mod project_import;
mod project_watch;
mod projects;
mod push;
mod row_session;
mod rows;
mod search;
mod settings;
mod shell;

use std::pin::Pin;

use gtl_application::viewer::{
    self,
    close_viewer_tabs::{self, CloseViewerTabs},
    move_viewer_tab, set_modified_files, set_viewer_tab_live, set_viewer_tab_pinned, work,
};
use gtl_models::{
    diffs::CommitId,
    failure::{Failure, Resource},
    viewer::ViewerTabId,
};
use gtl_wire::{
    proto,
    v1::{self, viewer_service_server::ViewerService},
    viewer::VIEWER_PROTOCOL_VERSION,
};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};

use self::{
    settings::load_user_settings,
    shell::{parse_identity, project_shell},
};
use super::{
    run_blocking,
    status::{GrpcResultExt as _, invalid_request, status},
};
use crate::{state::AppState, viewer_runtime};

pub(crate) struct ViewerGrpcService {
    state: AppState,
    server_info: ViewerServerInfo,
}

impl ViewerGrpcService {
    pub(crate) const fn new(state: AppState, server_info: ViewerServerInfo) -> Self {
        Self { state, server_info }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ViewerServerInfo {
    server_instance_id: String,
    protocol_version: u32,
}

impl ViewerServerInfo {
    pub(crate) fn generate() -> Self {
        Self {
            server_instance_id: uuid::Uuid::new_v4().to_string(),
            protocol_version: VIEWER_PROTOCOL_VERSION,
        }
    }
}

type WatchStream = Pin<Box<dyn Stream<Item = Result<v1::WatchViewerResponse, Status>> + Send>>;

#[tonic::async_trait]
impl ViewerService for ViewerGrpcService {
    async fn get_viewer_server_info(
        &self,
        _request: Request<v1::GetViewerServerInfoRequest>,
    ) -> Result<Response<v1::GetViewerServerInfoResponse>, Status> {
        Ok(Response::new(v1::GetViewerServerInfoResponse {
            server_instance_id: self.server_info.server_instance_id.clone(),
            protocol_version: self.server_info.protocol_version,
            server_version: env!("CARGO_PKG_VERSION").into(),
        }))
    }

    async fn get_viewer_push_availability(
        &self,
        request: Request<v1::GetViewerPushAvailabilityRequest>,
    ) -> Result<Response<v1::GetViewerPushAvailabilityResponse>, Status> {
        push::availability(&self.state, request).await
    }

    async fn create_viewer_push(
        &self,
        request: Request<v1::CreateViewerPushRequest>,
    ) -> Result<Response<v1::CreateViewerPushResponse>, Status> {
        push::create(&self.state, request).await
    }
    async fn get_viewer_push(
        &self,
        request: Request<v1::GetViewerPushRequest>,
    ) -> Result<Response<v1::GetViewerPushResponse>, Status> {
        push::get(&self.state, request)
    }
    async fn start_viewer_push(
        &self,
        request: Request<v1::StartViewerPushRequest>,
    ) -> Result<Response<v1::StartViewerPushResponse>, Status> {
        push::start(&self.state, request)
    }

    async fn get_viewer_file_filters(
        &self,
        request: Request<v1::GetViewerFileFiltersRequest>,
    ) -> Result<Response<v1::GetViewerFileFiltersResponse>, Status> {
        file_filters::get(&self.state, request).await
    }

    async fn set_viewer_file_filters(
        &self,
        request: Request<v1::SetViewerFileFiltersRequest>,
    ) -> Result<Response<v1::SetViewerFileFiltersResponse>, Status> {
        file_filters::set(&self.state, request).await
    }

    async fn get_viewer_project_status(
        &self,
        request: Request<v1::GetViewerProjectStatusRequest>,
    ) -> Result<Response<v1::GetViewerProjectStatusResponse>, Status> {
        projects::get_viewer_project_status(&self.state, request).await
    }

    async fn list_viewer_projects(
        &self,
        _request: Request<v1::ListViewerProjectsRequest>,
    ) -> Result<Response<v1::ListViewerProjectsResponse>, Status> {
        projects::list_viewer_projects(&self.state, _request).await
    }

    async fn discover_project_repositories(
        &self,
        request: Request<v1::DiscoverProjectRepositoriesRequest>,
    ) -> Result<Response<v1::DiscoverProjectRepositoriesResponse>, Status> {
        project_import::discover(&self.state, request).await
    }

    async fn import_project_repositories(
        &self,
        request: Request<v1::ImportProjectRepositoriesRequest>,
    ) -> Result<Response<v1::ImportProjectRepositoriesResponse>, Status> {
        project_import::import(&self.state, request).await
    }

    async fn open_viewer_project(
        &self,
        request: Request<v1::OpenViewerProjectRequest>,
    ) -> Result<Response<v1::OpenViewerProjectResponse>, Status> {
        projects::open_viewer_project(&self.state, request).await
    }

    async fn update_viewer_project(
        &self,
        request: Request<v1::UpdateViewerProjectRequest>,
    ) -> Result<Response<v1::UpdateViewerProjectResponse>, Status> {
        projects::update_viewer_project(&self.state, request).await
    }

    async fn open_unpushed_project_diffs(
        &self,
        request: Request<v1::OpenUnpushedProjectDiffsRequest>,
    ) -> Result<Response<v1::OpenUnpushedProjectDiffsResponse>, Status> {
        projects::open_unpushed_project_diffs(&self.state, request).await
    }

    async fn get_viewer_shell(
        &self,
        _request: Request<v1::GetViewerShellRequest>,
    ) -> Result<Response<v1::GetViewerShellResponse>, Status> {
        Ok(Response::new(v1::GetViewerShellResponse {
            shell: Some(project_shell(&self.state)?),
        }))
    }

    type WatchViewerStream = WatchStream;

    async fn watch_viewer(
        &self,
        request: Request<v1::WatchViewerRequest>,
    ) -> Result<Response<Self::WatchViewerStream>, Status> {
        let request = request.into_inner();
        let projects = request
            .project_ids
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid_request("project_ids"))?;
        let projects = gtl_wire::viewer::projects::ViewerProjectSelection::try_from(projects)
            .map_err(|_| invalid_request("project_ids"))?;
        let live_tab_id = request.live_tab_id.map(tab_id).transpose()?;
        let mut versions = self.state.viewer.subscribe();
        let (sender, receiver) = tokio::sync::mpsc::channel(8);
        if !projects.ids().is_empty() {
            project_watch::spawn(self.state.clone(), projects, sender.clone())?;
        }
        if let Some(tab_id) = live_tab_id {
            live_watch::spawn(self.state.clone(), tab_id, sender.clone());
        }
        tokio::spawn(async move {
            loop {
                let version = versions.borrow_and_update().value();
                if sender
                    .send(Ok(v1::WatchViewerResponse {
                        version,
                        live_check: None,
                        project_status: None,
                    }))
                    .await
                    .is_err()
                {
                    break;
                }
                tokio::select! {
                    () = sender.closed() => break,
                    changed = versions.changed() => if changed.is_err() { break; },
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(receiver))))
    }

    type StreamViewerRowSessionStream =
        ReceiverStream<Result<v1::StreamViewerRowSessionResponse, Status>>;

    async fn stream_viewer_row_session(
        &self,
        request: Request<tonic::Streaming<v1::StreamViewerRowSessionRequest>>,
    ) -> Result<Response<Self::StreamViewerRowSessionStream>, Status> {
        Ok(Response::new(
            row_session::start(self.state.clone(), request.into_inner()).await?,
        ))
    }

    type StreamViewerRowsStream = rows::RowStream;

    async fn stream_viewer_rows(
        &self,
        request: Request<v1::StreamViewerRowsRequest>,
    ) -> Result<Response<Self::StreamViewerRowsStream>, Status> {
        Ok(Response::new(rows::start(
            self.state.clone(),
            request.into_inner(),
        )?))
    }

    async fn activate_viewer_tab(
        &self,
        request: Request<v1::ActivateViewerTabRequest>,
    ) -> Result<Response<v1::ActivateViewerTabResponse>, Status> {
        let tab_id = tab_id(request.into_inner().tab_id)?;
        if let Some(work) = work::activate_tab(&self.state.viewer, tab_id).into_grpc()? {
            viewer_runtime::spawn_recipe(self.state.clone(), work);
        }
        Ok(Response::new(v1::ActivateViewerTabResponse {
            shell: Some(project_shell(&self.state)?),
        }))
    }

    async fn move_viewer_tab(
        &self,
        request: Request<v1::MoveViewerTabRequest>,
    ) -> Result<Response<v1::MoveViewerTabResponse>, Status> {
        let request = proto::viewer::decode_move_viewer_tab_request(request.into_inner())
            .map_err(|_| invalid_request("move"))?;
        move_viewer_tab::execute(request, &self.state.viewer).into_grpc()?;
        Ok(Response::new(v1::MoveViewerTabResponse {
            shell: Some(project_shell(&self.state)?),
        }))
    }

    async fn rename_viewer_snapshot(
        &self,
        request: Request<v1::RenameViewerSnapshotRequest>,
    ) -> Result<Response<v1::RenameViewerSnapshotResponse>, Status> {
        use viewer::rename_snapshot::{self, RenameSnapshotError};
        let request = request.into_inner();
        let request = gtl_wire::viewer::RenameViewerSnapshot {
            tab_id: tab_id(request.tab_id)?,
            name: request.name,
        };
        let state = self.state.clone();
        run_blocking(move || {
            let connection = state
                .database
                .connection_lock()
                .map_err(RenameSnapshotError::Unexpected)?;
            rename_snapshot::execute(&request, &state.viewer, &connection)
        })
        .await?
        .into_grpc()?;
        Ok(Response::new(v1::RenameViewerSnapshotResponse {}))
    }

    async fn set_viewer_tab_pinned(
        &self,
        request: Request<v1::SetViewerTabPinnedRequest>,
    ) -> Result<Response<v1::SetViewerTabPinnedResponse>, Status> {
        let request = request.into_inner();
        let request = gtl_wire::viewer::SetViewerTabPinned {
            tab_id: tab_id(request.tab_id)?,
            pinned: request.pinned,
        };
        set_viewer_tab_pinned::execute(request, &self.state.viewer).into_grpc()?;
        Ok(Response::new(v1::SetViewerTabPinnedResponse {}))
    }

    async fn close_other_viewer_tabs(
        &self,
        request: Request<v1::CloseOtherViewerTabsRequest>,
    ) -> Result<Response<v1::CloseOtherViewerTabsResponse>, Status> {
        let tab_id = tab_id(request.into_inner().tab_id)?;
        let closed =
            close_viewer_tabs::execute(CloseViewerTabs::Others(tab_id), &self.state.viewer)
                .into_grpc()?;
        if let Some(work) = closed {
            viewer_runtime::spawn_recipe(self.state.clone(), work);
        }
        Ok(Response::new(v1::CloseOtherViewerTabsResponse {}))
    }

    async fn close_viewer_tab(
        &self,
        request: Request<v1::CloseViewerTabRequest>,
    ) -> Result<Response<v1::CloseViewerTabResponse>, Status> {
        let tab_id = tab_id(request.into_inner().tab_id)?;
        let closed = close_viewer_tabs::execute(CloseViewerTabs::One(tab_id), &self.state.viewer)
            .into_grpc()?;
        if let Some(work) = closed {
            viewer_runtime::spawn_recipe(self.state.clone(), work);
        }
        Ok(Response::new(v1::CloseViewerTabResponse {
            shell: Some(project_shell(&self.state)?),
        }))
    }

    async fn refresh_viewer_tab(
        &self,
        request: Request<v1::RefreshViewerTabRequest>,
    ) -> Result<Response<v1::RefreshViewerTabResponse>, Status> {
        let tab_id = tab_id(request.into_inner().tab_id)?;
        let work = work::reserve_refresh(&self.state.viewer, tab_id).into_grpc()?;
        viewer_runtime::spawn_recipe(self.state.clone(), work);
        Ok(Response::new(v1::RefreshViewerTabResponse {
            shell: Some(project_shell(&self.state)?),
        }))
    }

    async fn update_viewer_tab(
        &self,
        request: Request<v1::UpdateViewerTabRequest>,
    ) -> Result<Response<v1::UpdateViewerTabResponse>, Status> {
        let tab_id = tab_id(request.into_inner().tab_id)?;
        let work = work::reserve_update(&self.state.viewer, tab_id).into_grpc()?;
        viewer_runtime::spawn_recipe(self.state.clone(), work);
        Ok(Response::new(v1::UpdateViewerTabResponse {
            shell: Some(project_shell(&self.state)?),
        }))
    }

    async fn set_viewer_tab_live(
        &self,
        request: Request<v1::SetViewerTabLiveRequest>,
    ) -> Result<Response<v1::SetViewerTabLiveResponse>, Status> {
        let request = request.into_inner();
        let request = gtl_wire::viewer::SetViewerTabLive {
            tab_id: tab_id(request.tab_id)?,
            live: request.live,
        };
        set_viewer_tab_live::execute(request, &self.state.viewer).into_grpc()?;
        Ok(Response::new(v1::SetViewerTabLiveResponse {
            shell: Some(project_shell(&self.state)?),
        }))
    }

    async fn set_viewer_changes_since(
        &self,
        request: Request<v1::SetViewerChangesSinceRequest>,
    ) -> Result<Response<v1::SetViewerChangesSinceResponse>, Status> {
        let request = proto::viewer::decode_set_viewer_changes_since_request(request.into_inner())
            .map_err(|error| invalid_request(error.field().unwrap_or("tab_id")))?;
        let work =
            work::reserve_changes_since(&self.state.viewer, request.tab_id, request.changes_since)
                .into_grpc()?;
        viewer_runtime::spawn_recipe(self.state.clone(), work);
        Ok(Response::new(v1::SetViewerChangesSinceResponse {
            shell: Some(project_shell(&self.state)?),
        }))
    }

    async fn select_viewer_commit(
        &self,
        request: Request<v1::SelectViewerCommitRequest>,
    ) -> Result<Response<v1::SelectViewerCommitResponse>, Status> {
        let request = request.into_inner();
        let tab_id = tab_id(request.tab_id)?;
        let commit_id =
            CommitId::try_from(request.commit_id).map_err(|_| invalid_request("commit_id"))?;
        let work = work::reserve_commit(&self.state.viewer, tab_id, &commit_id).into_grpc()?;
        viewer_runtime::spawn_commit(self.state.clone(), work);
        Ok(Response::new(v1::SelectViewerCommitResponse {
            shell: Some(project_shell(&self.state)?),
        }))
    }

    async fn set_viewer_modified_files(
        &self,
        request: Request<v1::SetViewerModifiedFilesRequest>,
    ) -> Result<Response<v1::SetViewerModifiedFilesResponse>, Status> {
        let request = request.into_inner();
        let request = gtl_wire::viewer::SetViewerModifiedFiles {
            tab_id: tab_id(request.tab_id)?,
            visible: request.visible,
        };
        let state = self.state.clone();
        super::run_blocking(move || {
            set_modified_files::execute(
                request,
                &state.viewer,
                &state.user_settings,
                &state.git,
                &state.database,
                &state.database,
            )
        })
        .await?
        .into_grpc()?;
        Ok(Response::new(v1::SetViewerModifiedFilesResponse {}))
    }

    async fn clear_viewer_commit_selection(
        &self,
        request: Request<v1::ClearViewerCommitSelectionRequest>,
    ) -> Result<Response<v1::ClearViewerCommitSelectionResponse>, Status> {
        let tab_id = tab_id(request.into_inner().tab_id)?;
        let cleared = work::clear_commit_selection(&self.state.viewer, tab_id).into_grpc()?;
        if !cleared {
            return Err(status(&Failure::Gone {
                resource: Resource::ViewerTab,
            }));
        }
        Ok(Response::new(v1::ClearViewerCommitSelectionResponse {
            shell: Some(project_shell(&self.state)?),
        }))
    }

    async fn set_viewer_preference(
        &self,
        request: Request<v1::SetViewerPreferenceRequest>,
    ) -> Result<Response<v1::SetViewerPreferenceResponse>, Status> {
        settings::set_viewer_preference(&self.state, request).await
    }

    async fn search_viewer_commits(
        &self,
        request: Request<v1::SearchViewerCommitsRequest>,
    ) -> Result<Response<v1::SearchViewerCommitsResponse>, Status> {
        use gtl_application::viewer::search_viewer_commits;
        let request = proto::viewer::decode_search_viewer_commits_request(request.into_inner())
            .map_err(|error| invalid_request(error.field().unwrap_or("scope")))?;
        let permit = self
            .state
            .viewer_project_status_workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| status(&Failure::Busy))?;
        let state = self.state.clone();
        let result = super::run_blocking(move || {
            let _permit = permit;
            search_viewer_commits::execute(
                &request,
                &state.viewer,
                &state.user_settings,
                &state.git,
            )
        })
        .await?
        .into_grpc()?;
        Ok(Response::new(
            proto::viewer::encode_search_viewer_commits_response(result),
        ))
    }

    async fn open_viewer_commit(
        &self,
        request: Request<v1::OpenViewerCommitRequest>,
    ) -> Result<Response<v1::OpenViewerCommitResponse>, Status> {
        use gtl_application::viewer::open_viewer_commit;
        let request = proto::viewer::decode_open_viewer_commit_request(request.into_inner())
            .map_err(|error| invalid_request(error.field().unwrap_or("scope")))?;
        let state = self.state.clone();
        let runtime = state.clone();
        let work = super::run_blocking(move || {
            open_viewer_commit::execute(request, &state.viewer, &state.git, &state.user_settings)
        })
        .await?
        .into_grpc()?;
        let tab_id = work.ticket().tab_id.into();
        viewer_runtime::spawn_recipe(runtime, work);
        Ok(Response::new(v1::OpenViewerCommitResponse { tab_id }))
    }

    async fn list_viewer_commits(
        &self,
        request: Request<v1::ListViewerCommitsRequest>,
    ) -> Result<Response<v1::ListViewerCommitsResponse>, Status> {
        let request = request.into_inner();
        let proto_identity = request
            .identity
            .ok_or_else(|| invalid_request("identity"))?;
        let identity = parse_identity(&proto_identity)?;
        let options = load_user_settings(&self.state)?.viewer_render_options();
        let source =
            viewer::shell::commit_source_for_identity(&self.state.viewer, identity, options)
                .into_grpc()?
                .ok_or_else(|| status(&Failure::Changed))?;
        Ok(Response::new(commits::page(
            proto_identity,
            &source.commits,
            request.cursor,
        )?))
    }

    async fn list_viewer_history(
        &self,
        request: Request<v1::ListViewerHistoryRequest>,
    ) -> Result<Response<v1::ListViewerHistoryResponse>, Status> {
        history::list_viewer_history(&self.state, request).await
    }

    async fn open_viewer_history(
        &self,
        request: Request<v1::OpenViewerHistoryRequest>,
    ) -> Result<Response<v1::OpenViewerHistoryResponse>, Status> {
        history::open_viewer_history(&self.state, request).await
    }

    async fn get_viewer_history_copy(
        &self,
        request: Request<v1::GetViewerHistoryCopyRequest>,
    ) -> Result<Response<v1::GetViewerHistoryCopyResponse>, Status> {
        history::get_viewer_history_copy(&self.state, request).await
    }

    async fn get_viewer_settings(
        &self,
        _request: Request<v1::GetViewerSettingsRequest>,
    ) -> Result<Response<v1::GetViewerSettingsResponse>, Status> {
        settings::get_viewer_settings(&self.state, _request)
    }

    async fn get_settings_recovery(
        &self,
        _request: Request<v1::GetSettingsRecoveryRequest>,
    ) -> Result<Response<v1::GetSettingsRecoveryResponse>, Status> {
        settings::get_settings_recovery(&self.state, _request).await
    }

    async fn reset_settings(
        &self,
        request: Request<v1::ResetSettingsRequest>,
    ) -> Result<Response<v1::ResetSettingsResponse>, Status> {
        settings::reset_settings(&self.state, request).await
    }

    async fn edit_settings(
        &self,
        request: Request<v1::EditSettingsRequest>,
    ) -> Result<Response<v1::EditSettingsResponse>, Status> {
        settings::edit_settings(&self.state, request).await
    }

    async fn search_viewer_files(
        &self,
        request: Request<v1::SearchViewerFilesRequest>,
    ) -> Result<Response<v1::SearchViewerFilesResponse>, Status> {
        search::search_viewer_files(&self.state, request).await
    }

    async fn find_viewer_diff(
        &self,
        request: Request<v1::FindViewerDiffRequest>,
    ) -> Result<Response<v1::FindViewerDiffResponse>, Status> {
        search::find_viewer_diff(&self.state, request).await
    }

    async fn read_viewer_diff_text(
        &self,
        request: Request<v1::ReadViewerDiffTextRequest>,
    ) -> Result<Response<v1::ReadViewerDiffTextResponse>, Status> {
        files::read_viewer_diff_text(&self.state, request).await
    }

    async fn open_viewer_diff_file(
        &self,
        request: Request<v1::OpenViewerDiffFileRequest>,
    ) -> Result<Response<v1::OpenViewerDiffFileResponse>, Status> {
        files::open_viewer_diff_file(&self.state, request).await
    }
}

fn tab_id(raw: u64) -> Result<ViewerTabId, Status> {
    ViewerTabId::try_new(raw).map_err(|_| invalid_request("tab_id"))
}
