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
    recipes::RecipeOp,
    settings::{
        get_user_settings::{self, GetUserSettings},
        set_setting_key,
    },
    viewer::{self, shell, work},
};
use gtl_models::{
    diffs::CommitId,
    settings::{SettingKeyValue, UserSettings},
    viewer::{DiffDensity, DiffLayout, RenderHistoryId, Theme, ViewerTabId},
};
use gtl_wire::{
    proto::viewer as viewer_proto,
    v1::{self, viewer_service_server::ViewerService},
    viewer::{
        SetViewerPreference, VIEWER_COMMIT_BODY_MAX_BYTES, VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES,
        VIEWER_COMMIT_PAGE_MAX_ENTRIES, ViewerDiffDensity, ViewerDiffExclusions, ViewerDiffLayout,
        ViewerFeedback, ViewerHistoryCursor, ViewerHistoryEntry, ViewerHistoryPage,
        ViewerProjectDiffExclusions, ViewerRecipeKind, ViewerShell, ViewerTheme,
        ViewerUserSettings, ViewerViewIdentity,
    },
};
use prost::Message as _;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};

use super::{run_blocking, settings::set_setting_key_error, unexpected, user_settings_load_error};
use crate::{state::AppState, viewer_runtime};

pub(crate) struct ViewerGrpcService {
    state: AppState,
}

impl ViewerGrpcService {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

type WatchStream = Pin<Box<dyn Stream<Item = Result<v1::WatchViewerResponse, Status>> + Send>>;

#[tonic::async_trait]
impl ViewerService for ViewerGrpcService {
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
                feedback,
            )
        })
        .map_err(|error| viewer_state_error(error, "project viewer shell"))?
        .map_err(|error| unexpected_viewer(error, "project viewer shell"))?;
    project_shell_proto(shell)
}

fn project_shell_proto(shell: ViewerShell) -> Result<v1::ViewerShell, Status> {
    viewer_proto::encode_viewer_shell(shell).map_err(|error| match error {
        viewer_proto::ViewerCodecError::Unrepresentable => {
            Status::resource_exhausted("viewer shell exceeds protobuf limits")
        }
        viewer_proto::ViewerCodecError::InvalidMessage => {
            Status::internal("viewer shell encoding failed")
        }
    })
}

pub(super) fn parse_identity(
    identity: &v1::ViewerViewIdentity,
) -> Result<ViewerViewIdentity, Status> {
    viewer_proto::decode_viewer_view_identity(*identity)
        .map_err(|_| Status::invalid_argument("viewer identity is invalid"))
}

fn preference(request: v1::SetViewerPreferenceRequest) -> Result<SettingKeyValue, Status> {
    let preference = viewer_proto::decode_set_viewer_preference_request(request)
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
    let request = viewer_proto::decode_list_viewer_history_request(request)
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
    viewer_proto::encode_list_viewer_history_response(page)
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
    viewer_proto::encode_get_viewer_settings_response(ViewerUserSettings {
        configuration_path,
        configured_theme,
        effective_theme: configured_theme.unwrap_or(ViewerTheme::Dark),
        render_options: viewer::project_render_options(settings.viewer_render_options()),
        push_confirmation_required: settings.push_confirmation_required(),
        diff_exclusions: ViewerDiffExclusions {
            default_extensions: exclusions.default_exclusions().clone(),
            projects: exclusions
                .project_exclusions()
                .map(|(project_name, extensions)| ViewerProjectDiffExclusions {
                    project_name: project_name.clone(),
                    extensions: extensions.clone(),
                })
                .collect(),
        },
    })
}

fn tab_id(raw: u64) -> Result<ViewerTabId, Status> {
    ViewerTabId::try_new(raw).map_err(|_| Status::invalid_argument("tab_id must be positive"))
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
