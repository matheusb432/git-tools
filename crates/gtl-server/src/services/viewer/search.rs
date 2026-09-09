use gtl_application::viewer::{find_viewer_diff, search_viewer_files};
use gtl_wire::{
    proto, v1,
    viewer::{
        VIEWER_FILE_SEARCH_MAX_ENCODED_BYTES, VIEWER_FILE_SEARCH_MAX_MATCHES,
        VIEWER_SEARCH_QUERY_MAX_BYTES,
    },
};
use prost::Message as _;
use tonic::{Request, Response, Status};

use super::{
    super::run_blocking,
    cancellation,
    errors::{self, unexpected_viewer},
};
use crate::state::AppState;

pub(super) async fn search_viewer_files(
    state: &AppState,
    request: Request<v1::SearchViewerFilesRequest>,
) -> Result<Response<v1::SearchViewerFilesResponse>, Status> {
    let request = proto::viewer::decode_search_viewer_files_request(request.into_inner())
        .map_err(|_| Status::invalid_argument("viewer file search is invalid"))?;
    validate_search_query(&request.query)?;
    let state = state.clone();
    let result = run_blocking(move || {
        search_viewer_files::execute(&request, &state.viewer, &state.user_settings)
    })
    .await?
    .map_err(errors::source)?;
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

pub(super) async fn find_viewer_diff(
    state: &AppState,
    request: Request<v1::FindViewerDiffRequest>,
) -> Result<Response<v1::FindViewerDiffResponse>, Status> {
    let request = proto::viewer::decode_find_viewer_diff_request(request.into_inner())
        .map_err(|_| Status::invalid_argument("viewer diff search is invalid"))?;
    validate_search_query(&request.query)?;
    let cancellation = state
        .viewer_searches
        .start_stream()
        .map_err(|error| unexpected_viewer(error, "start viewer search"))?;
    let _cancel_on_drop = cancellation::CancelOnDrop(cancellation.clone());
    let state = state.clone();
    let result = run_blocking(move || {
        find_viewer_diff::execute(&request, &state.viewer, &state.user_settings, &cancellation)
    })
    .await?
    .map_err(errors::find_diff)?;
    Ok(Response::new(
        proto::viewer::encode_find_viewer_diff_response(&result),
    ))
}

pub(super) fn validate_search_query(query: &str) -> Result<(), Status> {
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
