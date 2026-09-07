mod rows;

use std::pin::Pin;

use gtl_application::{
    diffs::open_diff_file_in_configured_editor::{self, OpenDiffFileInConfiguredEditorError},
    history::{
        RecentRenderRecord, copy_render, get_recent_render,
        list_recent_render_page::{
            self, ListRecentRenderPage, ListRecentRenderPageOk, RecentRenderPageCursor,
        },
    },
    live_views::delete_live_viewer_tab::{self, DeleteLiveViewerTabError},
    ports::UserSettingsEditError,
    projects::list_viewer_projects,
    recipes::RecipeOp,
    settings::{
        ProjectSettingsUpdate, ProjectSettingsUpdates, UserSettingsFieldUpdate, UserSettingsPatch,
        edit_settings::{self, EditSettingsError},
        get_user_settings::{self, GetUserSettings},
        set_setting_key,
    },
    viewer::{
        self,
        ensure_view_full_context::{self, EnsureViewFullContext, EnsureViewFullContextOk},
        find_viewer_diff,
        move_viewer_tab::{self, MoveViewerTabError},
        search_viewer_files, shell, work,
    },
};
use gtl_models::{
    diffs::CommitId,
    settings::{SettingKeyValue, UserSettings},
    viewer::{DiffDensity, DiffLayout, RenderHistoryId, Theme, ViewerTabId},
};
use gtl_wire::{
    proto,
    v1::{self, viewer_service_server::ViewerService},
    viewer::{
        EditSettingsRequest, FieldUpdate, SetViewerPreference, VIEWER_COMMIT_BODY_MAX_BYTES,
        VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES, VIEWER_COMMIT_PAGE_MAX_ENTRIES,
        VIEWER_FILE_SEARCH_MAX_ENCODED_BYTES, VIEWER_FILE_SEARCH_MAX_MATCHES,
        VIEWER_SEARCH_QUERY_MAX_BYTES, ViewerDiffDensity, ViewerDiffExclusions, ViewerDiffLayout,
        ViewerFeedback, ViewerHistoryCursor, ViewerHistoryEntry, ViewerHistoryPage,
        ViewerProjectDiffExclusions, ViewerRecipeKind, ViewerShell, ViewerTheme,
        ViewerUserSettings, ViewerViewIdentity,
    },
};
use prost::Message as _;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};

use super::{
    invalid_user_settings_configuration, run_blocking, settings::set_setting_key_error, unexpected,
    user_settings_load_error,
};
use crate::{state::AppState, viewer_runtime};

pub(crate) struct ViewerGrpcService {
    state: AppState,
}

impl ViewerGrpcService {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

fn project_catalogue_error(error: &gtl_application::ports::ProjectClientError) -> Status {
    tracing::warn!(error = ?error, "viewer project catalogue is unavailable");
    Status::failed_precondition("project catalogue is unavailable")
}

type WatchStream = Pin<Box<dyn Stream<Item = Result<v1::WatchViewerResponse, Status>> + Send>>;

#[tonic::async_trait]
impl ViewerService for ViewerGrpcService {
    async fn list_viewer_projects(
        &self,
        _request: Request<v1::ListViewerProjectsRequest>,
    ) -> Result<Response<v1::ListViewerProjectsResponse>, Status> {
        let repositories = self
            .state
            .projects
            .list_projects()
            .await
            .map_err(|error| project_catalogue_error(&error))?;
        let state = self.state.clone();
        let projects = run_blocking(move || {
            let connection = state.database.connection_lock()?;
            list_viewer_projects::execute(repositories, &state.git, &connection)
        })
        .await?
        .map_err(|error| unexpected(error, "list viewer projects"))?;
        Ok(Response::new(v1::ListViewerProjectsResponse {
            projects: projects
                .into_iter()
                .map(proto::viewer::projects::encode_project)
                .collect(),
        }))
    }

    async fn open_viewer_project(
        &self,
        request: Request<v1::OpenViewerProjectRequest>,
    ) -> Result<Response<v1::OpenViewerProjectResponse>, Status> {
        use gtl_application::projects::open_viewer_project::{
            self, OpenProjectComparison, OpenViewerProjectError,
        };
        let project = proto::viewer::projects::decode_open(request.into_inner())
            .map_err(|_| Status::invalid_argument("invalid project comparison"))?;
        let repositories = self
            .state
            .projects
            .list_projects()
            .await
            .map_err(|error| project_catalogue_error(&error))?;
        let state = self.state.clone();
        let work = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(OpenViewerProjectError::from)?;
            open_viewer_project::execute(
                OpenProjectComparison {
                    project,
                    repositories,
                },
                &state.git,
                &mut connection,
                &state.clock,
                &state.viewer,
            )
        })
        .await?
        .map_err(|error| match error {
            OpenViewerProjectError::NotFound => Status::not_found("project is no longer available"),
            OpenViewerProjectError::NoUpstream => {
                Status::failed_precondition("No upstream configured")
            }
            OpenViewerProjectError::Unexpected(error) => {
                unexpected(error, "open project comparison")
            }
        })?;
        let tab_id = work.ticket().tab_id.into();
        viewer_runtime::spawn_recipe(self.state.clone(), work);
        Ok(Response::new(v1::OpenViewerProjectResponse { tab_id }))
    }

    async fn get_viewer_shell(
        &self,
        _request: Request<v1::GetViewerShellRequest>,
    ) -> Result<Response<v1::GetViewerShellResponse>, Status> {
        Ok(Response::new(v1::GetViewerShellResponse {
            shell: Some(project_shell(&self.state, None)?),
        }))
    }

    type WatchViewerStream = WatchStream;

    async fn watch_viewer(
        &self,
        _request: Request<v1::WatchViewerRequest>,
    ) -> Result<Response<Self::WatchViewerStream>, Status> {
        let mut versions = self.state.viewer.subscribe();
        let (sender, receiver) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            loop {
                let version = versions.borrow_and_update().value();
                if sender
                    .send(Ok(v1::WatchViewerResponse { version }))
                    .await
                    .is_err()
                {
                    break;
                }
                if versions.changed().await.is_err() {
                    break;
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(receiver))))
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
        if let Some(work) = work::activate_tab(&self.state.viewer, tab_id)
            .map_err(|error| map_reserve_recipe(error, "activate viewer tab"))?
        {
            viewer_runtime::spawn_recipe(self.state.clone(), work);
        }
        Ok(Response::new(v1::ActivateViewerTabResponse {
            shell: Some(project_shell(&self.state, None)?),
        }))
    }

    async fn move_viewer_tab(
        &self,
        request: Request<v1::MoveViewerTabRequest>,
    ) -> Result<Response<v1::MoveViewerTabResponse>, Status> {
        let request = proto::viewer::decode_move_viewer_tab_request(request.into_inner())
            .map_err(|_| Status::invalid_argument("move viewer tab request is invalid"))?;
        move_viewer_tab::execute(request, &self.state.viewer).map_err(move_viewer_tab_error)?;
        Ok(Response::new(v1::MoveViewerTabResponse {
            shell: Some(project_shell(&self.state, None)?),
        }))
    }

    async fn close_viewer_tab(
        &self,
        request: Request<v1::CloseViewerTabRequest>,
    ) -> Result<Response<v1::CloseViewerTabResponse>, Status> {
        let tab_id = tab_id(request.into_inner().tab_id)?;
        if let Some(work) = work::close_tab(&self.state.viewer, tab_id)
            .map_err(|error| map_reserve_recipe(error, "close viewer tab"))?
        {
            viewer_runtime::spawn_recipe(self.state.clone(), work);
        }
        Ok(Response::new(v1::CloseViewerTabResponse {
            shell: Some(project_shell(&self.state, None)?),
        }))
    }

    async fn refresh_viewer_tab(
        &self,
        request: Request<v1::RefreshViewerTabRequest>,
    ) -> Result<Response<v1::RefreshViewerTabResponse>, Status> {
        let tab_id = tab_id(request.into_inner().tab_id)?;
        let work = work::reserve_refresh(&self.state.viewer, tab_id)
            .map_err(|error| map_reserve_recipe(error, "refresh viewer tab"))?;
        viewer_runtime::spawn_recipe(self.state.clone(), work);
        Ok(Response::new(v1::RefreshViewerTabResponse {
            shell: Some(project_shell(&self.state, None)?),
        }))
    }

    async fn delete_live_viewer_tab(
        &self,
        request: Request<v1::DeleteLiveViewerTabRequest>,
    ) -> Result<Response<v1::DeleteLiveViewerTabResponse>, Status> {
        let tab_id = tab_id(request.into_inner().tab_id)?;
        let state = self.state.clone();
        let deletion = run_blocking(move || {
            let connection = state.database.connection_lock()?;
            Ok::<_, anyhow::Error>(delete_live_viewer_tab::execute(
                tab_id,
                &connection,
                &state.viewer,
            ))
        })
        .await?
        .map_err(|error| unexpected(error, "open live-view database"))?;
        if let Some(work) = deletion.map_err(delete_live_viewer_tab_error)? {
            viewer_runtime::spawn_recipe(self.state.clone(), work);
        }
        Ok(Response::new(v1::DeleteLiveViewerTabResponse {
            shell: Some(project_shell(
                &self.state,
                Some(ViewerFeedback::LiveViewDeleted),
            )?),
        }))
    }

    async fn select_viewer_commit(
        &self,
        request: Request<v1::SelectViewerCommitRequest>,
    ) -> Result<Response<v1::SelectViewerCommitResponse>, Status> {
        let request = request.into_inner();
        let tab_id = tab_id(request.tab_id)?;
        let commit_id = CommitId::try_from(request.commit_id)
            .map_err(|_| Status::invalid_argument("commit_id is invalid"))?;
        let work = work::reserve_commit(&self.state.viewer, tab_id, &commit_id)
            .map_err(map_reserve_commit)?;
        viewer_runtime::spawn_commit(self.state.clone(), work);
        Ok(Response::new(v1::SelectViewerCommitResponse {
            shell: Some(project_shell(&self.state, None)?),
        }))
    }

    async fn clear_viewer_commit_selection(
        &self,
        request: Request<v1::ClearViewerCommitSelectionRequest>,
    ) -> Result<Response<v1::ClearViewerCommitSelectionResponse>, Status> {
        let tab_id = tab_id(request.into_inner().tab_id)?;
        let cleared = work::clear_commit_selection(&self.state.viewer, tab_id)
            .map_err(|error| viewer_state_error(error, "clear viewer commit selection"))?;
        if !cleared {
            return Err(Status::not_found("viewer tab is not available"));
        }
        Ok(Response::new(v1::ClearViewerCommitSelectionResponse {
            shell: Some(project_shell(&self.state, None)?),
        }))
    }

    async fn set_viewer_preference(
        &self,
        request: Request<v1::SetViewerPreferenceRequest>,
    ) -> Result<Response<v1::SetViewerPreferenceResponse>, Status> {
        let mutation = preference(request.into_inner())?;
        let loads_full_context = matches!(&mutation, SettingKeyValue::Density(DiffDensity::Full));
        let mut store = self.state.user_settings.clone();
        let viewer = self.state.viewer.clone();
        let change = run_blocking(move || set_setting_key::execute(mutation, &mut store, &viewer))
            .await?
            .map_err(set_setting_key_error)?;
        if change.viewer_rows_changed {
            self.state
                .viewer_row_streams
                .cancel_current_stream()
                .map_err(|error| unexpected_viewer(error, "cancel viewer row stream"))?;
        }
        if loads_full_context && change.viewer_rows_changed {
            let state = self.state.clone();
            let loaded = run_blocking(move || {
                ensure_view_full_context::execute(
                    EnsureViewFullContext::Active,
                    &state.viewer,
                    &state.git,
                )
            })
            .await?
            .map_err(|error| unexpected_viewer(error, "load full-context viewer source"))?;
            if matches!(loaded, EnsureViewFullContextOk::Stale) {
                tracing::debug!("full-context viewer source became stale before publication");
            }
        }
        Ok(Response::new(v1::SetViewerPreferenceResponse {
            shell: Some(project_shell(&self.state, None)?),
        }))
    }

    async fn list_viewer_commits(
        &self,
        request: Request<v1::ListViewerCommitsRequest>,
    ) -> Result<Response<v1::ListViewerCommitsResponse>, Status> {
        let request = request.into_inner();
        let proto_identity = request
            .identity
            .ok_or_else(|| Status::invalid_argument("identity is required"))?;
        let identity = parse_identity(&proto_identity)?;
        let options = load_user_settings(&self.state)?.viewer_render_options();
        let commits = shell::commits_for_identity(&self.state.viewer, identity, options)
            .map_err(|error| viewer_state_error(error, "list viewer commits"))?
            .ok_or_else(|| Status::aborted("viewer identity changed"))?;
        let cursor = usize::try_from(request.cursor.unwrap_or_default())
            .map_err(|_| Status::invalid_argument("cursor is invalid"))?;
        if cursor > commits.len() {
            return Err(Status::invalid_argument(
                "cursor is outside the commit list",
            ));
        }
        let mut response = v1::ListViewerCommitsResponse {
            identity: Some(proto_identity),
            commits: Vec::new(),
            next_cursor: None,
        };
        let mut index = cursor;
        while index < commits.len() && response.commits.len() < VIEWER_COMMIT_PAGE_MAX_ENTRIES {
            let commit = &commits[index];
            let mut projected =
                commit_summary(commit, commit.body.len() > VIEWER_COMMIT_BODY_MAX_BYTES);
            response.commits.push(projected.clone());
            if response.encoded_len() > VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES {
                response.commits.pop();
                if response.commits.is_empty() && !projected.body_omitted {
                    projected.body.clear();
                    projected.body_omitted = true;
                    response.commits.push(projected);
                    if response.encoded_len() > VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES {
                        return Err(Status::resource_exhausted(
                            "one commit summary exceeds the viewer page limit",
                        ));
                    }
                    index += 1;
                }
                break;
            }
            index += 1;
        }
        response.next_cursor = (index < commits.len())
            .then(|| u32::try_from(index))
            .transpose()
            .map_err(|_| Status::resource_exhausted("commit cursor exceeds u32"))?;
        Ok(Response::new(response))
    }

    async fn list_viewer_history(
        &self,
        request: Request<v1::ListViewerHistoryRequest>,
    ) -> Result<Response<v1::ListViewerHistoryResponse>, Status> {
        let cursor = history_cursor(request.into_inner())?;
        let state = self.state.clone();
        let page = run_blocking(move || {
            let connection = state.database.connection_lock()?;
            list_recent_render_page::execute(ListRecentRenderPage { cursor }, &connection)
                .map_err(anyhow::Error::from)
        })
        .await?
        .map_err(|error| unexpected(error, "list viewer history"))?;
        Ok(Response::new(project_history_page(page)?))
    }

    async fn open_viewer_history(
        &self,
        request: Request<v1::OpenViewerHistoryRequest>,
    ) -> Result<Response<v1::OpenViewerHistoryResponse>, Status> {
        let record = history_record(&self.state, request.into_inner().render_id).await?;
        let work = work::reserve_history_open(&self.state.viewer, record)
            .map_err(|error| map_reserve_recipe(error, "open viewer history"))?;
        viewer_runtime::spawn_recipe(self.state.clone(), work);
        Ok(Response::new(v1::OpenViewerHistoryResponse {
            shell: Some(project_shell(&self.state, None)?),
        }))
    }

    async fn get_viewer_history_copy(
        &self,
        request: Request<v1::GetViewerHistoryCopyRequest>,
    ) -> Result<Response<v1::GetViewerHistoryCopyResponse>, Status> {
        let record = history_record(&self.state, request.into_inner().render_id).await?;
        let json = copy_render::format(&record)
            .map_err(|error| unexpected(error, "format viewer history copy"))?;
        Ok(Response::new(v1::GetViewerHistoryCopyResponse { json }))
    }

    async fn get_viewer_settings(
        &self,
        _request: Request<v1::GetViewerSettingsRequest>,
    ) -> Result<Response<v1::GetViewerSettingsResponse>, Status> {
        let settings = load_user_settings(&self.state)?;
        Ok(Response::new(project_settings(
            &settings,
            self.state
                .user_settings
                .path()
                .map(|path| path.display().to_string()),
        )))
    }

    async fn edit_settings(
        &self,
        request: Request<v1::EditSettingsRequest>,
    ) -> Result<Response<v1::EditSettingsResponse>, Status> {
        let request = proto::viewer::decode_edit_settings_request(request.into_inner())
            .map_err(|_| Status::invalid_argument("settings patch is invalid"))?;
        let loads_full_context = matches!(
            request.density,
            FieldUpdate::Update(ViewerDiffDensity::Full)
        );
        let request = application_settings_request(request)?;
        let mut store = self.state.user_settings.clone();
        let viewer = self.state.viewer.clone();
        let change = run_blocking(move || edit_settings::execute(request, &mut store, &viewer))
            .await?
            .map_err(edit_settings_error)?;
        if change.viewer_rows_changed {
            self.state
                .viewer_row_streams
                .cancel_current_stream()
                .map_err(|error| unexpected_viewer(error, "cancel viewer row stream"))?;
        }
        if loads_full_context && change.viewer_rows_changed {
            let state = self.state.clone();
            run_blocking(move || {
                ensure_view_full_context::execute(
                    EnsureViewFullContext::Active,
                    &state.viewer,
                    &state.git,
                )
            })
            .await?
            .map_err(|error| unexpected_viewer(error, "load full-context viewer source"))?;
        }
        Ok(Response::new(v1::EditSettingsResponse {}))
    }

    async fn search_viewer_files(
        &self,
        request: Request<v1::SearchViewerFilesRequest>,
    ) -> Result<Response<v1::SearchViewerFilesResponse>, Status> {
        let request = proto::viewer::decode_search_viewer_files_request(request.into_inner())
            .map_err(|_| Status::invalid_argument("viewer file search is invalid"))?;
        validate_search_query(&request.query)?;
        let options = load_user_settings(&self.state)?.viewer_render_options();
        let snapshot =
            shell::content_snapshot_for_identity(&self.state.viewer, request.identity, options)
                .map_err(|error| viewer_state_error(error, "search viewer files"))?
                .ok_or_else(|| Status::aborted("viewer identity changed"))?;
        let view = snapshot.shared_view();
        let result = run_blocking(move || search_viewer_files::execute(&request, &view)).await?;
        if result.files.len() > VIEWER_FILE_SEARCH_MAX_MATCHES {
            return Err(Status::resource_exhausted(
                "viewer file search has too many matches",
            ));
        }
        let response = proto::viewer::encode_search_viewer_files_response(result);
        if response.encoded_len() > VIEWER_FILE_SEARCH_MAX_ENCODED_BYTES {
            return Err(Status::resource_exhausted(
                "viewer file search exceeds the response limit",
            ));
        }
        Ok(Response::new(response))
    }

    async fn find_viewer_diff(
        &self,
        request: Request<v1::FindViewerDiffRequest>,
    ) -> Result<Response<v1::FindViewerDiffResponse>, Status> {
        let request = proto::viewer::decode_find_viewer_diff_request(request.into_inner())
            .map_err(|_| Status::invalid_argument("viewer diff search is invalid"))?;
        validate_search_query(&request.query)?;
        let options = load_user_settings(&self.state)?.viewer_render_options();
        let view = if request.identity.render_options.density == ViewerDiffDensity::Full {
            let state = self.state.clone();
            let identity = request.identity;
            let loaded = run_blocking(move || {
                ensure_view_full_context::execute(
                    EnsureViewFullContext::Identity {
                        identity,
                        render_options: options,
                    },
                    &state.viewer,
                    &state.git,
                )
            })
            .await?
            .map_err(|error| unexpected_viewer(error, "load full-context viewer search"))?;
            match loaded {
                EnsureViewFullContextOk::Ready(view) => view,
                EnsureViewFullContextOk::Stale => {
                    return Err(Status::aborted("viewer identity changed"));
                }
            }
        } else {
            shell::content_snapshot_for_identity(&self.state.viewer, request.identity, options)
                .map_err(|error| viewer_state_error(error, "search viewer diff"))?
                .ok_or_else(|| Status::aborted("viewer identity changed"))?
                .shared_view()
        };
        let result = run_blocking(move || find_viewer_diff::execute(&request, &view))
            .await?
            .map_err(|_| Status::resource_exhausted("viewer diff search exceeds server limits"))?;
        Ok(Response::new(
            proto::viewer::encode_find_viewer_diff_response(&result),
        ))
    }

    async fn open_viewer_diff_file(
        &self,
        request: Request<v1::OpenViewerDiffFileRequest>,
    ) -> Result<Response<v1::OpenViewerDiffFileResponse>, Status> {
        let request = request.into_inner();
        let identity = parse_identity(
            request
                .identity
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("identity is required"))?,
        )?;
        let options = load_user_settings(&self.state)?.viewer_render_options();
        let file_id = gtl_wire::viewer::ViewerDiffFileId::try_from(request.file_id)
            .map_err(|_| Status::invalid_argument("file_id is invalid"))?;
        let (view, path) =
            shell::diff_file_for_identity(&self.state.viewer, identity, options, &file_id)
                .map_err(|error| viewer_state_error(error, "load viewer diff file"))?
                .ok_or_else(|| Status::aborted("viewer identity changed"))?;
        let state = self.state.clone();
        run_blocking(move || {
            open_diff_file_in_configured_editor::execute(
                &path,
                &view,
                &state.file_system,
                &state.text_editor,
            )
        })
        .await?
        .map_err(open_file_error)?;
        Ok(Response::new(v1::OpenViewerDiffFileResponse {}))
    }
}

fn application_field_update<Input, Output>(
    update: FieldUpdate<Input>,
    map: impl FnOnce(Input) -> Output,
) -> UserSettingsFieldUpdate<Output> {
    match update {
        FieldUpdate::Update(value) => UserSettingsFieldUpdate::Update(map(value)),
        FieldUpdate::Clear => UserSettingsFieldUpdate::Clear,
        FieldUpdate::Unchanged => UserSettingsFieldUpdate::Unchanged,
    }
}

fn application_settings_request(request: EditSettingsRequest) -> Result<UserSettingsPatch, Status> {
    let projects = match request.projects {
        FieldUpdate::Update(projects) => {
            let mut mapped = Vec::with_capacity(projects.len());
            for project in projects {
                mapped.push(ProjectSettingsUpdate {
                    name: project.project_name,
                    excluded_from_push_all: project.excluded_from_push_all,
                    diff_exclusions: project.diff_exclusions,
                });
            }
            UserSettingsFieldUpdate::Update(ProjectSettingsUpdates::try_new(mapped).map_err(
                |error| {
                    Status::invalid_argument(format!(
                        "project settings contain duplicate name `{}`",
                        error.name()
                    ))
                },
            )?)
        }
        FieldUpdate::Clear => UserSettingsFieldUpdate::Clear,
        FieldUpdate::Unchanged => UserSettingsFieldUpdate::Unchanged,
    };
    Ok(UserSettingsPatch {
        theme: application_field_update(request.theme, |value| match value {
            ViewerTheme::Light => Theme::Light,
            ViewerTheme::Dark => Theme::Dark,
            ViewerTheme::Hearth => Theme::Hearth,
            ViewerTheme::Mirage => Theme::Mirage,
            ViewerTheme::Glacier => Theme::Glacier,
            ViewerTheme::Noir => Theme::Noir,
            ViewerTheme::Graphite => Theme::Graphite,
        }),
        layout: application_field_update(request.layout, |value| match value {
            ViewerDiffLayout::Unified => DiffLayout::Unified,
            ViewerDiffLayout::Split => DiffLayout::Split,
        }),
        density: application_field_update(request.density, |value| match value {
            ViewerDiffDensity::Compact => DiffDensity::Compact,
            ViewerDiffDensity::Full => DiffDensity::Full,
        }),
        push_confirmation_required: application_field_update(
            request.push_confirmation_required,
            |value| value,
        ),
        default_diff_exclusions: application_field_update(
            request.default_diff_exclusions,
            |value| value,
        ),
        projects,
    })
}

fn edit_settings_error(error: EditSettingsError) -> Status {
    match error {
        EditSettingsError::Settings(error) => match error {
            UserSettingsEditError::InvalidConfiguration(error) => {
                invalid_user_settings_configuration(&error, "edit settings")
            }
            UserSettingsEditError::Conflict(_) => {
                Status::aborted("user settings edit conflicted with another writer")
            }
            UserSettingsEditError::Adapter(error) => unexpected(error, "edit settings"),
        },
        EditSettingsError::ViewerState(error) => unexpected(error, "edit settings"),
    }
}

pub(super) fn load_user_settings(state: &AppState) -> Result<UserSettings, Status> {
    get_user_settings::execute(GetUserSettings, &state.user_settings).map_err(|error| match error {
        get_user_settings::GetUserSettingsError::Settings(error) => user_settings_load_error(error),
    })
}

fn project_shell(
    state: &AppState,
    feedback: Option<ViewerFeedback>,
) -> Result<v1::ViewerShell, Status> {
    let settings = load_user_settings(state)?;
    let shell = state
        .viewer
        .inspect(|session| {
            shell::project(
                session,
                settings.viewer_render_options(),
                settings.theme().unwrap_or(Theme::Dark),
                settings.viewer_keybindings(),
                feedback,
            )
        })
        .map_err(|error| viewer_state_error(error, "project viewer shell"))?
        .map_err(|error| unexpected_viewer(error, "project viewer shell"))?;
    project_shell_proto(shell)
}

fn project_shell_proto(shell: ViewerShell) -> Result<v1::ViewerShell, Status> {
    proto::viewer::encode_viewer_shell(shell).map_err(|error| match error {
        proto::viewer::ViewerCodecError::Unrepresentable => {
            Status::resource_exhausted("viewer shell exceeds protobuf limits")
        }
        proto::viewer::ViewerCodecError::InvalidMessage => {
            Status::internal("viewer shell encoding failed")
        }
    })
}

pub(super) fn parse_identity(
    identity: &v1::ViewerViewIdentity,
) -> Result<ViewerViewIdentity, Status> {
    proto::viewer::decode_viewer_view_identity(*identity)
        .map_err(|_| Status::invalid_argument("viewer identity is invalid"))
}

fn preference(request: v1::SetViewerPreferenceRequest) -> Result<SettingKeyValue, Status> {
    let preference = proto::viewer::decode_set_viewer_preference_request(request)
        .map_err(|_| Status::invalid_argument("viewer preference is invalid"))?;
    Ok(match preference {
        SetViewerPreference::Layout(layout) => SettingKeyValue::Layout(match layout {
            ViewerDiffLayout::Unified => DiffLayout::Unified,
            ViewerDiffLayout::Split => DiffLayout::Split,
        }),
        SetViewerPreference::Density(density) => SettingKeyValue::Density(match density {
            ViewerDiffDensity::Compact => DiffDensity::Compact,
            ViewerDiffDensity::Full => DiffDensity::Full,
        }),
        SetViewerPreference::Theme(theme) => SettingKeyValue::Theme(match theme {
            ViewerTheme::Dark => Theme::Dark,
            ViewerTheme::Light => Theme::Light,
            ViewerTheme::Hearth => Theme::Hearth,
            ViewerTheme::Mirage => Theme::Mirage,
            ViewerTheme::Glacier => Theme::Glacier,
            ViewerTheme::Noir => Theme::Noir,
            ViewerTheme::Graphite => Theme::Graphite,
        }),
    })
}

fn commit_summary(
    commit: &gtl_models::diffs::Commit,
    body_omitted: bool,
) -> v1::ViewerCommitSummary {
    v1::ViewerCommitSummary {
        id: commit.id.to_string(),
        subject: commit.subject.clone(),
        body: if body_omitted {
            String::new()
        } else {
            commit.body.clone()
        },
        committed_at: commit.committed_at.as_ref().to_owned(),
        is_merge: commit.is_merge(),
        body_omitted,
    }
}

fn history_cursor(request: v1::ListViewerHistoryRequest) -> Result<RecentRenderPageCursor, Status> {
    let request = proto::viewer::decode_list_viewer_history_request(request)
        .map_err(|_| Status::invalid_argument("history cursor is invalid"))?;
    Ok(match request.cursor {
        ViewerHistoryCursor::Newest => RecentRenderPageCursor::Newest,
        ViewerHistoryCursor::Oldest => RecentRenderPageCursor::Oldest,
        ViewerHistoryCursor::OlderThan { render_id, page } => RecentRenderPageCursor::OlderThan {
            render: render_id,
            page,
        },
        ViewerHistoryCursor::NewerThan { render_id, page } => RecentRenderPageCursor::NewerThan {
            render: render_id,
            page,
        },
    })
}

fn project_history_page(
    page: ListRecentRenderPageOk,
) -> Result<v1::ListViewerHistoryResponse, Status> {
    let page = ViewerHistoryPage {
        entries: page
            .entries
            .into_iter()
            .map(|record| ViewerHistoryEntry {
                id: record.id,
                title: record.title,
                repository_name: record.repo_name,
                kind: match record.recipe.op {
                    RecipeOp::Diff { .. } => ViewerRecipeKind::Diff,
                    RecipeOp::MergeDiff { .. } => ViewerRecipeKind::MergeDiff,
                },
                range_label: record.range_label,
                rendered_at: record.rendered_at,
            })
            .collect(),
        total_count: page.total_count,
        position: page.position,
        has_newer: page.has_newer,
        has_older: page.has_older,
    };
    proto::viewer::encode_list_viewer_history_response(page)
        .map_err(|_| Status::internal("stored viewer history is invalid"))
}

async fn history_record(state: &AppState, raw_id: u64) -> Result<RecentRenderRecord, Status> {
    let id = render_history_id(raw_id)?;
    let state = state.clone();
    run_blocking(move || {
        let connection = state.database.connection_lock()?;
        get_recent_render::execute(&get_recent_render::GetRecentRender { id }, &connection)
            .map_err(anyhow::Error::from)
    })
    .await?
    .map_err(|error| unexpected(error, "load viewer history entry"))?
    .ok_or_else(|| Status::not_found("viewer history entry is not available"))
}

fn project_settings(
    settings: &UserSettings,
    configuration_path: Option<String>,
) -> v1::GetViewerSettingsResponse {
    let configured_theme = settings.theme().map(viewer::project_theme);
    let exclusions = settings.diff_exclusions();
    proto::viewer::encode_get_viewer_settings_response(ViewerUserSettings {
        configuration_path,
        configured_theme,
        effective_theme: configured_theme.unwrap_or(ViewerTheme::Dark),
        render_options: viewer::project_render_options(settings.viewer_render_options()),
        push_confirmation_required: settings.push_confirmation_required(),
        diff_exclusions: ViewerDiffExclusions {
            default_extensions: exclusions.default_exclusions().clone(),
            projects: {
                let mut names = std::collections::BTreeSet::new();
                names.extend(
                    exclusions
                        .project_exclusions()
                        .map(|(name, _)| name.clone()),
                );
                names.extend(settings.push_all_exclusions().projects().cloned());
                names
                    .into_iter()
                    .map(|project_name| ViewerProjectDiffExclusions {
                        extensions: exclusions
                            .for_project(&project_name)
                            .cloned()
                            .unwrap_or_default(),
                        excluded_from_push_all: settings
                            .push_all_exclusions()
                            .contains(&project_name),
                        project_name,
                    })
                    .collect()
            },
        },
    })
}

fn tab_id(raw: u64) -> Result<ViewerTabId, Status> {
    ViewerTabId::try_new(raw).map_err(|_| Status::invalid_argument("tab_id must be positive"))
}

fn validate_search_query(query: &str) -> Result<(), Status> {
    if query.len() > VIEWER_SEARCH_QUERY_MAX_BYTES {
        return Err(Status::invalid_argument("search query is too long"));
    }
    if query.chars().any(char::is_control) {
        return Err(Status::invalid_argument(
            "search query contains control characters",
        ));
    }
    Ok(())
}

fn render_history_id(raw: u64) -> Result<RenderHistoryId, Status> {
    let raw = i64::try_from(raw).map_err(|_| Status::invalid_argument("render_id is invalid"))?;
    RenderHistoryId::try_new(raw)
        .map_err(|_| Status::invalid_argument("render_id must be positive"))
}

fn map_reserve_recipe(error: viewer::work::ReserveRecipeError, operation: &'static str) -> Status {
    match error {
        viewer::work::ReserveRecipeError::UnknownTab => {
            Status::not_found("viewer tab is not available")
        }
        error => unexpected_viewer(error, operation),
    }
}

fn move_viewer_tab_error(error: MoveViewerTabError) -> Status {
    match error {
        MoveViewerTabError::UnknownTab => Status::not_found("viewer tab is not available"),
        MoveViewerTabError::State(error) => viewer_state_error(error, "move viewer tab"),
    }
}

fn delete_live_viewer_tab_error(error: DeleteLiveViewerTabError) -> Status {
    match error {
        DeleteLiveViewerTabError::UnknownTab => {
            Status::not_found("live viewer tab is not available")
        }
        DeleteLiveViewerTabError::ReserveWork(error) => {
            map_reserve_recipe(error, "refresh viewer after deleting live tab")
        }
        error => unexpected_viewer(error, "delete live viewer tab"),
    }
}

fn map_reserve_commit(error: viewer::work::ReserveCommitError) -> Status {
    match error {
        viewer::work::ReserveCommitError::Selection(
            viewer::session::BeginCommitSelectionError::UnknownTab
            | viewer::session::BeginCommitSelectionError::UnknownCommit,
        ) => Status::not_found("viewer commit is not available"),
        viewer::work::ReserveCommitError::Selection(
            viewer::session::BeginCommitSelectionError::StaleRange
            | viewer::session::BeginCommitSelectionError::SelectionPending,
        ) => Status::aborted("viewer selection changed"),
        error => unexpected_viewer(error, "select viewer commit"),
    }
}

fn open_file_error(error: OpenDiffFileInConfiguredEditorError) -> Status {
    match error {
        OpenDiffFileInConfiguredEditorError::FileNotInCurrentDiff
        | OpenDiffFileInConfiguredEditorError::DiffFileDeleted
        | OpenDiffFileInConfiguredEditorError::DiffFileUnavailable => {
            Status::failed_precondition(error.to_string())
        }
        OpenDiffFileInConfiguredEditorError::DiffFileOutsideRepository => {
            Status::permission_denied("diff file resolves outside its repository")
        }
        error => unexpected(error, "open viewer diff file"),
    }
}

pub(super) fn viewer_state_error(
    error: viewer::ViewerStateError,
    operation: &'static str,
) -> Status {
    unexpected_viewer(error, operation)
}

pub(super) fn unexpected_viewer(
    error: impl std::fmt::Debug + std::fmt::Display,
    operation: &'static str,
) -> Status {
    tracing::error!(error = ?error, operation, "viewer operation failed");
    Status::internal("viewer operation failed")
}

#[cfg(test)]
mod tests {
    use tonic::Code;

    use super::*;

    #[test]
    fn viewer_search_query_accepts_the_wire_limit() {
        assert!(validate_search_query(&"x".repeat(VIEWER_SEARCH_QUERY_MAX_BYTES)).is_ok());
    }

    #[test]
    fn viewer_search_query_rejects_oversize_or_control_text() {
        assert_eq!(
            validate_search_query(&"x".repeat(VIEWER_SEARCH_QUERY_MAX_BYTES + 1))
                .unwrap_err()
                .code(),
            Code::InvalidArgument
        );
        assert_eq!(
            validate_search_query("line\nnext").unwrap_err().code(),
            Code::InvalidArgument
        );
    }
}
