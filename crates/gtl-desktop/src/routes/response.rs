use maud::{DOCTYPE, html};
use tauri::http::{HeaderValue, Response, StatusCode, header};

use super::view_snapshot;
use crate::{
    materialization::MaterializationError,
    recipes::{RecipeError, SelectCommitError},
    render::VIEW_STATE_ERROR,
    session::PendingRecipesError,
};

const HTML_CONTENT_TYPE: &str = "text/html; charset=utf-8";
const TEXT_CONTENT_TYPE: &str = "text/plain; charset=utf-8";
const DYNAMIC_CACHE_CONTROL: &str = "no-store";
const RECOVERY_HEADER: &str = "x-gtl-recovery";
const RECOVERY_RESWAP: &str = "outerHTML";

#[derive(Debug, Clone, Copy)]
pub(super) enum ErrorTarget {
    Document,
    View,
    Tabs,
    History,
    Action,
}

#[derive(Debug)]
pub(super) enum RouteError {
    Internal(String),
    Conflict,
    NotFound,
}

impl From<String> for RouteError {
    fn from(value: String) -> Self {
        Self::Internal(value)
    }
}

impl From<view_snapshot::RenderError> for RouteError {
    fn from(value: view_snapshot::RenderError) -> Self {
        match value {
            view_snapshot::RenderError::State(reason) => Self::Internal(reason),
            view_snapshot::RenderError::Retry | view_snapshot::RenderError::Conflict => {
                Self::Conflict
            }
        }
    }
}

impl From<PendingRecipesError> for RouteError {
    fn from(value: PendingRecipesError) -> Self {
        Self::Internal(value.to_string())
    }
}

impl From<MaterializationError> for RouteError {
    fn from(value: MaterializationError) -> Self {
        match value {
            MaterializationError::Conflict => Self::Conflict,
            MaterializationError::ExhaustedIds
            | MaterializationError::Render(_)
            | MaterializationError::StatePoisoned => Self::Internal(value.to_string()),
        }
    }
}

impl From<RecipeError> for RouteError {
    fn from(value: RecipeError) -> Self {
        match value {
            RecipeError::Failed(reason) => Self::Internal(reason),
            RecipeError::Stale => Self::Conflict,
        }
    }
}

impl From<SelectCommitError> for RouteError {
    fn from(value: SelectCommitError) -> Self {
        match value {
            SelectCommitError::Reserve(
                crate::session::BeginCommitSelectionError::UnknownTab
                | crate::session::BeginCommitSelectionError::UnknownCommit,
            ) => Self::NotFound,
            SelectCommitError::Reserve(
                crate::session::BeginCommitSelectionError::StaleRange
                | crate::session::BeginCommitSelectionError::SelectionPending,
            ) => Self::Conflict,
            SelectCommitError::Failed(reason) => Self::Internal(reason),
        }
    }
}

pub(super) enum RouteOutput {
    Html(String),
    Empty(StatusCode),
}

pub(super) type RouteResult = Result<RouteOutput, RouteError>;

pub(super) fn into_response(output: RouteOutput) -> Response<Vec<u8>> {
    match output {
        RouteOutput::Html(body) => {
            response(StatusCode::OK, HTML_CONTENT_TYPE, body.into_bytes(), false)
        }
        RouteOutput::Empty(status) => response(status, TEXT_CONTENT_TYPE, Vec::new(), false),
    }
}

pub(super) fn error_response(target: ErrorTarget, error: &RouteError) -> Response<Vec<u8>> {
    match error {
        RouteError::Internal(reason) => eprintln!("gtl-viewer route failure: {reason}"),
        RouteError::Conflict => eprintln!("gtl-viewer route failure: fragment generation conflict"),
        RouteError::NotFound => {}
    }
    let status = match error {
        RouteError::Conflict => StatusCode::CONFLICT,
        RouteError::NotFound => StatusCode::NOT_FOUND,
        RouteError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let body = match target {
        ErrorTarget::Document => error_document(),
        ErrorTarget::View => error_view(),
        ErrorTarget::Tabs => error_tabs(),
        ErrorTarget::History => error_history(),
        ErrorTarget::Action => String::new(),
    };
    response(
        status,
        HTML_CONTENT_TYPE,
        body.into_bytes(),
        !matches!(target, ErrorTarget::Document | ErrorTarget::Action),
    )
}

fn response(
    status: StatusCode,
    content_type: &'static str,
    body: Vec<u8>,
    recovery: bool,
) -> Response<Vec<u8>> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(DYNAMIC_CACHE_CONTROL),
    );
    if recovery {
        response.headers_mut().insert(
            header::HeaderName::from_static(RECOVERY_HEADER),
            HeaderValue::from_static("true"),
        );
        response.headers_mut().insert(
            header::HeaderName::from_static("hx-reswap"),
            HeaderValue::from_static(RECOVERY_RESWAP),
        );
    }
    response
}

fn error_document() -> String {
    html! {
        (DOCTYPE)
        html lang="en" { head { meta charset="utf-8"; title { "git-tools viewer error" } }
            body { main role="alert" { h1 { "The viewer could not complete this request" } p { "Please retry the operation." } } }
        }
    }.into_string()
}

fn error_view() -> String {
    html! {
        section id="viewer-view" class="viewer-view min-h-0 min-w-0 overflow-hidden [&.htmx-swapping]:bg-acc-soft [&.htmx-settling]:bg-acc-soft" data-viewer-state=(VIEW_STATE_ERROR) {
            div class="viewer-status grid min-h-full grid-cols-[minmax(0,520px)] place-content-center p-8 text-ink-2" role="alert" {
                strong class="text-ink" { "The view could not be updated" }
                p class="mt-1 mb-0" { "Please retry the operation." }
            }
        }
    }
    .into_string()
}

fn error_tabs() -> String {
    html! {
        nav id="viewer-tabs" class="viewer-tabs z-[70] flex min-w-0 items-end gap-2.5 border-b border-line bg-surface px-3 pt-2 [&.htmx-swapping]:border-acc-line [&.htmx-settling]:border-acc-line mobile:px-2" aria-label="Open diffs" {
            div class="mb-2 rounded-sm border border-del-line bg-del-bg px-2.5 py-1.5 text-xs text-del" role="alert" {
                strong { "The tabs could not be updated" }
                span { " Please retry the operation." }
            }
        }
    }
    .into_string()
}

fn error_history() -> String {
    html! {
        section id="viewer-history" class="viewer-history gtl-scroll h-[calc(100%-58px)] overflow-auto px-4 py-3.5 [&.htmx-swapping]:bg-acc-soft [&.htmx-settling]:bg-acc-soft" {
            div class="rounded-sm border border-del-line bg-del-bg px-2.5 py-2 text-xs text-del" role="alert" {
                "History could not be loaded. Please retry."
            }
        }
    }
    .into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_and_internal_errors_keep_target_roots_and_hide_details() {
        for (target, error, status, root) in [
            (
                ErrorTarget::View,
                RouteError::Conflict,
                StatusCode::CONFLICT,
                "<section id=\"viewer-view\"",
            ),
            (
                ErrorTarget::Tabs,
                RouteError::Internal("sqlite /secret/path".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
                "<nav id=\"viewer-tabs\"",
            ),
        ] {
            let response = error_response(target, &error);
            let html = String::from_utf8_lossy(response.body());
            assert_eq!(response.status(), status);
            assert_eq!(response.headers()["X-GTL-Recovery"], "true");
            assert_eq!(response.headers()["HX-Reswap"], "outerHTML");
            assert!(html.starts_with(root));
            assert!(!html.contains("sqlite"));
            assert!(!html.contains("/secret/path"));
        }
    }

    #[test]
    fn action_errors_return_empty_non_swappable_responses() {
        for (error, expected_status) in [
            (RouteError::NotFound, StatusCode::NOT_FOUND),
            (
                RouteError::Internal("editor /secret/path".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ] {
            let response = error_response(ErrorTarget::Action, &error);
            assert_eq!(response.status(), expected_status);
            assert!(response.body().is_empty());
            assert!(!response.headers().contains_key("X-GTL-Recovery"));
            assert!(!response.headers().contains_key("HX-Reswap"));
        }
    }

    #[test]
    fn recovery_fragments_own_their_error_presentation() {
        let view = error_view();
        let tabs = error_tabs();
        let history = error_history();

        assert!(view.contains(&format!("data-viewer-state=\"{VIEW_STATE_ERROR}\"")));
        assert!(view.contains("class=\"viewer-status "));
        assert!(tabs.contains("class=\"viewer-tabs "));
        assert!(tabs.contains("border-del-line"));
        assert!(history.contains("class=\"viewer-history "));
        assert!(history.contains("border-del-line"));
    }
}
