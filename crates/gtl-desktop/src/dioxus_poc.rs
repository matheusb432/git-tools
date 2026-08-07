use serde::Serialize;
use tauri::{
    State,
    http::{Method, Request, Response, StatusCode},
};

use crate::{presentation::ViewerApp, protocol_config, routes};

const SHADOW_HOST_CSS: &str = ":host{display:block;height:100%;min-height:0}:host>div{height:100%;min-height:0}#viewer-view{height:100%;min-height:0}";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiffFragment {
    html: String,
    css: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "fragment", rename_all = "kebab-case")]
pub(crate) enum DiffFragmentState {
    Pending,
    Ready(DiffFragment),
}

fn app_request(path: &str) -> Result<Request<Vec<u8>>, String> {
    Request::builder()
        .method(Method::GET)
        .uri(format!(
            "{}{path}",
            protocol_config::APP_URL.trim_end_matches('/')
        ))
        .body(Vec::new())
        .map_err(|error| error.to_string())
}

fn adapt_preview_css(css: &str) -> String {
    let mut adapted = css.replace(":root", ":host");
    adapted.push_str(SHADOW_HOST_CSS);
    adapted
}

fn classify_fragment(response: Response<Vec<u8>>) -> Result<DiffFragmentState, String> {
    match response.status() {
        StatusCode::NO_CONTENT => Ok(DiffFragmentState::Pending),
        StatusCode::OK => {
            let html = String::from_utf8(response.into_body())
                .map_err(|_| "the backend diff fragment was not valid UTF-8".to_owned())?;
            Ok(DiffFragmentState::Ready(DiffFragment {
                html,
                css: adapt_preview_css(gtl_preview::preview_css()),
            }))
        }
        status => Err(format!(
            "the backend diff fragment route returned HTTP {}",
            status.as_u16()
        )),
    }
}

#[tauri::command]
pub(crate) async fn dioxus_poc_process_pending(app: State<'_, ViewerApp>) -> Result<(), String> {
    let app = app.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let response = routes::serve_app(&app, app_request("/pending")?);
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "the backend pending route returned HTTP {}",
                response.status().as_u16()
            ))
        }
    })
    .await
    .map_err(|error| format!("the backend pending worker could not be joined: {error}"))?
}

#[tauri::command]
pub(crate) async fn dioxus_poc_diff_fragment(
    app: State<'_, ViewerApp>,
) -> Result<DiffFragmentState, String> {
    let app = app.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        classify_fragment(routes::serve_unmaterialized_view(&app))
    })
    .await
    .map_err(|error| format!("the backend diff worker could not be joined: {error}"))?
}

#[cfg(test)]
mod tests {
    use tauri::http::{Method, Response, StatusCode};

    use super::{
        DiffFragment, DiffFragmentState, adapt_preview_css, app_request, classify_fragment,
    };

    #[test]
    fn app_requests_remain_on_the_existing_custom_protocol_origin() {
        let request = app_request("/ready").expect("build internal request");

        assert_eq!(request.method(), Method::GET);
        assert_eq!(request.uri().to_string(), "gtl://app/ready");
    }

    #[test]
    fn preview_css_targets_the_shadow_host_instead_of_the_document_root() {
        let adapted = adapt_preview_css(":root,[data-theme=dark]{--bg:black}");

        assert!(adapted.starts_with(":host,[data-theme=dark]{--bg:black}"));
        assert!(adapted.contains(":host{display:block;height:100%;min-height:0}"));
        assert!(!adapted.contains(":root"));
    }

    #[test]
    fn fragment_response_distinguishes_pending_ready_and_failure() {
        let pending = Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Vec::new())
            .expect("pending response");
        assert_eq!(
            classify_fragment(pending).expect("classify pending response"),
            DiffFragmentState::Pending
        );

        let ready = Response::builder()
            .status(StatusCode::OK)
            .body(b"<section id=\"viewer-view\"></section>".to_vec())
            .expect("ready response");
        let result = classify_fragment(ready).expect("classify ready response");
        assert!(matches!(
            result,
            DiffFragmentState::Ready(DiffFragment { ref html, ref css })
                if html.contains("viewer-view") && css.contains(":host")
        ));

        let failed = Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Vec::new())
            .expect("failed response");
        let error = classify_fragment(failed).expect_err("failed route is rejected");
        assert!(error.contains("500"));
    }
}
