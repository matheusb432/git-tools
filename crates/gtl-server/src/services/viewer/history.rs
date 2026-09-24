use gtl_application::{
    history::{
        RecentRenderRecord, copy_render, get_recent_render,
        list_recent_render_page::{
            self, ListRecentRenderPage, ListRecentRenderPageOk, RecentRenderPageCursor,
        },
    },
    recipes::RecipeOp,
    viewer::work,
};
use gtl_models::{
    failure::{ErrorClass, Failure, Resource},
    viewer::RenderHistoryId,
};
use gtl_wire::{
    proto, v1,
    viewer::{ViewerHistoryCursor, ViewerHistoryEntry, ViewerHistoryPage, ViewerRecipeKind},
};
use tonic::{Request, Response, Status};

use super::{
    super::{
        run_blocking,
        status::{GrpcResultExt as _, invalid_request, private, status},
        unexpected,
    },
    shell::project_shell,
};
use crate::{state::AppState, viewer_runtime};

pub(super) async fn list_viewer_history(
    state: &AppState,
    request: Request<v1::ListViewerHistoryRequest>,
) -> Result<Response<v1::ListViewerHistoryResponse>, Status> {
    let request = request.into_inner();
    let decoded = proto::viewer::decode_list_viewer_history_request(request)
        .map_err(|_| invalid_request("filter"))?;
    let cursor = history_cursor(decoded.cursor);
    let state = state.clone();
    let page = run_blocking(move || {
        let connection = state.database.connection_lock()?;
        list_recent_render_page::execute(
            &ListRecentRenderPage {
                cursor,
                filter: decoded.filter,
            },
            &connection,
        )
        .map_err(anyhow::Error::from)
    })
    .await?
    .map_err(|error| unexpected(error, "list viewer history"))?;
    Ok(Response::new(project_history_page(page)?))
}

pub(super) async fn open_viewer_history(
    state: &AppState,
    request: Request<v1::OpenViewerHistoryRequest>,
) -> Result<Response<v1::OpenViewerHistoryResponse>, Status> {
    let record = history_record(state, request.into_inner().render_id).await?;
    let work = work::reserve_history_open(&state.viewer, record).into_grpc()?;
    viewer_runtime::spawn_recipe(state.clone(), work);
    Ok(Response::new(v1::OpenViewerHistoryResponse {
        shell: Some(project_shell(state, None)?),
    }))
}

pub(super) async fn get_viewer_history_copy(
    state: &AppState,
    request: Request<v1::GetViewerHistoryCopyRequest>,
) -> Result<Response<v1::GetViewerHistoryCopyResponse>, Status> {
    let record = history_record(state, request.into_inner().render_id).await?;
    let json = copy_render::format(&record)
        .map_err(|error| unexpected(error, "format viewer history copy"))?;
    Ok(Response::new(v1::GetViewerHistoryCopyResponse { json }))
}

fn history_cursor(cursor: ViewerHistoryCursor) -> RecentRenderPageCursor {
    match cursor {
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
    }
}

pub(super) fn project_history_page(
    page: ListRecentRenderPageOk,
) -> Result<v1::ListViewerHistoryResponse, Status> {
    let page = ViewerHistoryPage {
        projects: page.projects,
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
        .map_err(|_| private(ErrorClass::DataLoss, "stored viewer history is invalid"))
}

pub(super) async fn history_record(
    state: &AppState,
    raw_id: u64,
) -> Result<RecentRenderRecord, Status> {
    let id = render_history_id(raw_id)?;
    let state = state.clone();
    run_blocking(move || {
        let connection = state.database.connection_lock()?;
        get_recent_render::execute(&get_recent_render::GetRecentRender { id }, &connection)
            .map_err(anyhow::Error::from)
    })
    .await?
    .map_err(|error| unexpected(error, "load viewer history entry"))?
    .ok_or_else(|| {
        status(&Failure::Gone {
            resource: Resource::Snapshot,
        })
    })
}

pub(super) fn render_history_id(raw: u64) -> Result<RenderHistoryId, Status> {
    let raw = i64::try_from(raw).map_err(|_| invalid_request("render_id"))?;
    RenderHistoryId::try_new(raw).map_err(|_| invalid_request("render_id"))
}
