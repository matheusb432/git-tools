use std::{
    collections::HashMap,
    num::{NonZeroU64, NonZeroUsize},
    path::PathBuf,
};

use gtl_application::{
    history::list_recent_render_page::RecentRenderPageCursor,
    viewer::{DiffDensity, DiffLayout, RenderHistoryId, RenderOptions, Theme, ViewerTabId},
};
use tauri::http::{Method, Request, StatusCode};

use crate::{materialization::ViewLoadId, protocol_config};

const OPEN_DIFF_FILE_QUERY_BYTES_MAX: usize = 16 * 1024;
const HISTORY_QUERY_BYTES_MAX: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingChange {
    Layout(DiffLayout),
    Density(DiffDensity),
    Theme(Theme),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResumeNonce(NonZeroU64);

impl ResumeNonce {
    pub(crate) fn try_new(value: u64) -> Option<Self> {
        NonZeroU64::new(value).map(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Route {
    Document {
        resume: Option<ResumeNonce>,
    },
    View {
        tab: ViewerTabId,
        options: RenderOptions,
    },
    CommitPatch {
        tab: ViewerTabId,
        sha: String,
        options: RenderOptions,
    },
    Refresh {
        tab: ViewerTabId,
    },
    Close {
        tab: ViewerTabId,
    },
    DeleteLiveView {
        tab: ViewerTabId,
    },
    Activate {
        tab: ViewerTabId,
    },
    History {
        cursor: RecentRenderPageCursor,
    },
    OpenHistory {
        render: RenderHistoryId,
    },
    Settings(SettingChange),
    Pending,
    Ready,
    LoadNext {
        load: ViewLoadId,
    },
    OpenDiffFile {
        tab: ViewerTabId,
        diff_file_path: PathBuf,
    },
}

pub(crate) fn parse(request: &Request<Vec<u8>>) -> Result<Route, StatusCode> {
    let uri = request.uri();
    let authority = uri.authority().map(tauri::http::uri::Authority::as_str);
    let configured_origin = uri.scheme_str() == Some(protocol_config::PROTOCOL_SCHEME)
        && authority == Some(protocol_config::APP_HOST);
    if !configured_origin {
        return Err(StatusCode::NOT_FOUND);
    }

    let segments = uri
        .path()
        .strip_prefix('/')
        .unwrap_or(uri.path())
        .split('/')
        .collect::<Vec<_>>();
    let shape = match segments.as_slice() {
        [""] => RouteShape::Document,
        ["tabs", _, "view"] => RouteShape::View,
        ["tabs", _, "commits", _, "view"] => RouteShape::CommitPatch,
        ["tabs", _, "refresh"] => RouteShape::Refresh,
        ["tabs", _, "close"] => RouteShape::Close,
        ["tabs", _, "live-view"] => RouteShape::DeleteLiveView,
        ["tabs", _, "activate"] => RouteShape::Activate,
        ["tabs", _, "files", "open"] => RouteShape::OpenDiffFile,
        ["history"] => RouteShape::History,
        ["history", _, "open"] => RouteShape::OpenHistory,
        ["settings"] => RouteShape::Settings,
        ["pending"] => RouteShape::Pending,
        ["ready"] => RouteShape::Ready,
        ["loads", _, "next"] => RouteShape::LoadNext,
        _ => return Err(StatusCode::NOT_FOUND),
    };
    let expected_method = match shape {
        RouteShape::DeleteLiveView => Method::DELETE,
        RouteShape::OpenDiffFile => Method::POST,
        _ => Method::GET,
    };
    if request.method() != expected_method {
        return Err(StatusCode::METHOD_NOT_ALLOWED);
    }

    match shape {
        RouteShape::Document => {
            let resume = parse_resume(uri.query())?;
            Ok(Route::Document { resume })
        }
        RouteShape::View => Ok(Route::View {
            tab: parse_tab_id(segments[1])?,
            options: parse_render_options(uri.query())?,
        }),
        RouteShape::CommitPatch => {
            let sha = segments[3];
            if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(StatusCode::BAD_REQUEST);
            }
            Ok(Route::CommitPatch {
                tab: parse_tab_id(segments[1])?,
                sha: sha.to_ascii_lowercase(),
                options: parse_render_options(uri.query())?,
            })
        }
        RouteShape::Refresh => tab_route(uri.query(), segments[1], |tab| Route::Refresh { tab }),
        RouteShape::Close => tab_route(uri.query(), segments[1], |tab| Route::Close { tab }),
        RouteShape::DeleteLiveView => tab_route(uri.query(), segments[1], |tab| {
            Route::DeleteLiveView { tab }
        }),
        RouteShape::Activate => tab_route(uri.query(), segments[1], |tab| Route::Activate { tab }),
        RouteShape::OpenDiffFile => parse_open_diff_file(uri.query(), segments[1]),
        RouteShape::History => {
            let cursor = parse_history_cursor(uri.query())?;
            Ok(Route::History { cursor })
        }
        RouteShape::OpenHistory => {
            reject_query(uri.query())?;
            let raw = segments[1]
                .parse::<i64>()
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let render = RenderHistoryId::try_new(raw).map_err(|_| StatusCode::BAD_REQUEST)?;
            Ok(Route::OpenHistory { render })
        }
        RouteShape::Settings => parse_settings(uri.query()),
        RouteShape::Pending => {
            reject_query(uri.query())?;
            Ok(Route::Pending)
        }
        RouteShape::Ready => {
            reject_query(uri.query())?;
            Ok(Route::Ready)
        }
        RouteShape::LoadNext => {
            reject_query(uri.query())?;
            let value = segments[1]
                .parse::<u64>()
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let load = ViewLoadId::try_new(value).ok_or(StatusCode::BAD_REQUEST)?;
            Ok(Route::LoadNext { load })
        }
    }
}

fn parse_resume(query: Option<&str>) -> Result<Option<ResumeNonce>, StatusCode> {
    let Some(query) = query else {
        return Ok(None);
    };
    let query = parse_query(Some(query))?;
    if query.len() != 1 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let raw = query.get("resume").ok_or(StatusCode::BAD_REQUEST)?;
    let value = raw.parse::<u64>().map_err(|_| StatusCode::BAD_REQUEST)?;
    ResumeNonce::try_new(value)
        .map(Some)
        .ok_or(StatusCode::BAD_REQUEST)
}

fn parse_history_cursor(query: Option<&str>) -> Result<RecentRenderPageCursor, StatusCode> {
    let Some(raw_query) = query else {
        return Ok(RecentRenderPageCursor::Newest);
    };
    if raw_query.len() > HISTORY_QUERY_BYTES_MAX {
        return Err(StatusCode::BAD_REQUEST);
    }
    let query = parse_query(Some(raw_query))?;
    if query.len() == 1 && query.get("edge") == Some(&"last") {
        return Ok(RecentRenderPageCursor::Oldest);
    }
    if query.len() != 2 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let page = query
        .get("page")
        .ok_or(StatusCode::BAD_REQUEST)?
        .parse::<usize>()
        .map_err(|_| StatusCode::BAD_REQUEST)
        .and_then(|page| NonZeroUsize::new(page).ok_or(StatusCode::BAD_REQUEST))?;
    let (key, older) = if let Some(value) = query.get("before") {
        (value, true)
    } else if let Some(value) = query.get("after") {
        (value, false)
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let render = key
        .parse::<i64>()
        .map_err(|_| StatusCode::BAD_REQUEST)
        .and_then(|id| RenderHistoryId::try_new(id).map_err(|_| StatusCode::BAD_REQUEST))?;
    Ok(if older {
        RecentRenderPageCursor::OlderThan { render, page }
    } else {
        RecentRenderPageCursor::NewerThan { render, page }
    })
}

#[derive(Debug, Clone, Copy)]
enum RouteShape {
    Document,
    View,
    CommitPatch,
    Refresh,
    Close,
    DeleteLiveView,
    Activate,
    OpenDiffFile,
    History,
    OpenHistory,
    Settings,
    Pending,
    Ready,
    LoadNext,
}

fn parse_render_options(query: Option<&str>) -> Result<RenderOptions, StatusCode> {
    let query = parse_query(query)?;
    if query.len() != 2 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let layout = query.get("layout").ok_or(StatusCode::BAD_REQUEST)?;
    let density = query.get("density").ok_or(StatusCode::BAD_REQUEST)?;
    RenderOptions::try_from((*layout, *density)).map_err(|_| StatusCode::BAD_REQUEST)
}

fn tab_route(
    query: Option<&str>,
    raw_tab: &str,
    route: impl FnOnce(ViewerTabId) -> Route,
) -> Result<Route, StatusCode> {
    reject_query(query)?;
    Ok(route(parse_tab_id(raw_tab)?))
}

fn parse_tab_id(raw: &str) -> Result<ViewerTabId, StatusCode> {
    raw.parse::<u64>()
        .map_err(|_| StatusCode::BAD_REQUEST)
        .and_then(|id| ViewerTabId::try_new(id).map_err(|_| StatusCode::BAD_REQUEST))
}

fn parse_settings(query: Option<&str>) -> Result<Route, StatusCode> {
    let query = parse_query(query)?;
    if query.len() != 1 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let setting = if let Some(value) = query.get("layout") {
        SettingChange::Layout(value.parse().map_err(|_| StatusCode::BAD_REQUEST)?)
    } else if let Some(value) = query.get("density") {
        SettingChange::Density(value.parse().map_err(|_| StatusCode::BAD_REQUEST)?)
    } else if let Some(value) = query.get("theme") {
        SettingChange::Theme(value.parse().map_err(|_| StatusCode::BAD_REQUEST)?)
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };
    Ok(Route::Settings(setting))
}

fn parse_open_diff_file(query: Option<&str>, raw_tab: &str) -> Result<Route, StatusCode> {
    let query = query.ok_or(StatusCode::BAD_REQUEST)?;
    if query.len() > OPEN_DIFF_FILE_QUERY_BYTES_MAX {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut pairs = url::form_urlencoded::parse(query.as_bytes());
    let (key, value) = pairs.next().ok_or(StatusCode::BAD_REQUEST)?;
    if pairs.next().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if key != "path" || value.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(Route::OpenDiffFile {
        tab: parse_tab_id(raw_tab)?,
        diff_file_path: PathBuf::from(value.as_ref()),
    })
}

fn reject_query(query: Option<&str>) -> Result<(), StatusCode> {
    if query.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

fn parse_query(query: Option<&str>) -> Result<HashMap<&str, &str>, StatusCode> {
    let query = query.ok_or(StatusCode::BAD_REQUEST)?;
    if query.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut values = HashMap::new();
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').ok_or(StatusCode::BAD_REQUEST)?;
        if key.is_empty() || value.is_empty() || values.insert(key, value).is_some() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroUsize, path::PathBuf};

    use gtl_application::{
        history::list_recent_render_page::RecentRenderPageCursor,
        viewer::{DiffDensity, DiffLayout, RenderHistoryId, RenderOptions, ViewerTabId},
    };
    use tauri::http::{Method, Request, StatusCode};

    use super::{OPEN_DIFF_FILE_QUERY_BYTES_MAX, ResumeNonce, Route, parse};
    use crate::{materialization::ViewLoadId, protocol_config};

    fn app_uri(path_and_query: &str) -> String {
        format!(
            "{}{}",
            protocol_config::APP_URL,
            path_and_query.trim_start_matches('/')
        )
    }

    fn request(method: Method, uri: &str) -> Request<Vec<u8>> {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(Vec::new())
            .expect("test request builds")
    }

    #[test]
    fn parses_only_known_same_origin_routes() {
        let tab = ViewerTabId::try_new(7).expect("positive id");
        let render = RenderHistoryId::try_new(42).expect("positive id");
        let sha = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        for (uri, method, expected) in [
            (app_uri("/"), Method::GET, Route::Document { resume: None }),
            (
                app_uri("/tabs/7/view?layout=split&density=full"),
                Method::GET,
                Route::View {
                    tab,
                    options: RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
                },
            ),
            (
                app_uri(&format!(
                    "/tabs/7/commits/{sha}/view?layout=split&density=full"
                )),
                Method::GET,
                Route::CommitPatch {
                    tab,
                    sha: sha.to_ascii_lowercase(),
                    options: RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
                },
            ),
            (
                app_uri("/tabs/7/refresh"),
                Method::GET,
                Route::Refresh { tab },
            ),
            (
                app_uri("/tabs/7/live-view"),
                Method::DELETE,
                Route::DeleteLiveView { tab },
            ),
            (
                app_uri("/tabs/7/files/open?path=src%2Fa+b.rs"),
                Method::POST,
                Route::OpenDiffFile {
                    tab,
                    diff_file_path: PathBuf::from("src/a b.rs"),
                },
            ),
            (
                app_uri("/history"),
                Method::GET,
                Route::History {
                    cursor: RecentRenderPageCursor::Newest,
                },
            ),
            (
                app_uri("/history?before=42&page=2"),
                Method::GET,
                Route::History {
                    cursor: RecentRenderPageCursor::OlderThan {
                        render,
                        page: NonZeroUsize::new(2).expect("positive page"),
                    },
                },
            ),
            (
                app_uri("/history?after=42&page=1"),
                Method::GET,
                Route::History {
                    cursor: RecentRenderPageCursor::NewerThan {
                        render,
                        page: NonZeroUsize::new(1).expect("positive page"),
                    },
                },
            ),
            (
                app_uri("/history?edge=last"),
                Method::GET,
                Route::History {
                    cursor: RecentRenderPageCursor::Oldest,
                },
            ),
            (app_uri("/ready"), Method::GET, Route::Ready),
            (
                app_uri("/loads/9/next"),
                Method::GET,
                Route::LoadNext {
                    load: ViewLoadId::try_new(9).expect("positive load id"),
                },
            ),
            (
                app_uri("/history/42/open"),
                Method::GET,
                Route::OpenHistory { render },
            ),
        ] {
            assert_eq!(parse(&request(method, &uri)), Ok(expected));
        }
    }

    #[test]
    fn parses_only_a_single_positive_resume_nonce_on_the_document_route() {
        assert_eq!(
            parse(&request(Method::GET, &app_uri("/?resume=7"))),
            Ok(Route::Document {
                resume: Some(ResumeNonce::try_new(7).expect("positive nonce")),
            })
        );

        for query in [
            "?resume=",
            "?resume=0",
            "?resume=abc",
            "?resume=1&resume=2",
            "?unknown=1",
            "?resume=1&unknown=2",
        ] {
            assert_eq!(
                parse(&request(Method::GET, &app_uri(&format!("/{query}")))),
                Err(StatusCode::BAD_REQUEST),
                "{query}"
            );
        }
    }

    #[test]
    fn history_cursor_rejects_offsets_and_malformed_keysets() {
        for route in [
            "/history?offset=30",
            "/history?before=42",
            "/history?before=42&page=0",
            "/history?before=0&page=2",
            "/history?after=42&page=abc",
            "/history?before=42&after=41&page=2",
            "/history?edge=first",
            "/history?edge=middle",
            "/history?edge=last&page=2",
        ] {
            assert_eq!(
                parse(&request(Method::GET, &app_uri(route))),
                Err(StatusCode::BAD_REQUEST),
                "{route}"
            );
        }
    }

    #[test]
    fn open_diff_file_requires_post_one_positive_tab_and_one_nonempty_path() {
        assert_eq!(
            parse(&request(
                Method::GET,
                &app_uri("/tabs/7/files/open?path=src%2Fa.rs")
            )),
            Err(StatusCode::METHOD_NOT_ALLOWED)
        );

        for route in [
            "/tabs/0/files/open?path=src%2Fa.rs",
            "/tabs/7/files/open",
            "/tabs/7/files/open?path=",
            "/tabs/7/files/open?path=src%2Fa.rs&path=src%2Fb.rs",
            "/tabs/7/files/open?unknown=value",
        ] {
            assert_eq!(
                parse(&request(Method::POST, &app_uri(route))),
                Err(StatusCode::BAD_REQUEST),
                "{route}"
            );
        }
    }

    #[test]
    fn open_diff_file_query_accepts_the_limit_and_rejects_one_byte_over() {
        let query_prefix = "path=";
        let value = "a".repeat(OPEN_DIFF_FILE_QUERY_BYTES_MAX - query_prefix.len());
        let route = format!("/tabs/7/files/open?{query_prefix}{value}");

        assert_eq!(
            parse(&request(Method::POST, &app_uri(&route))),
            Ok(Route::OpenDiffFile {
                tab: ViewerTabId::try_new(7).expect("positive id"),
                diff_file_path: PathBuf::from(&value),
            })
        );

        let route = format!("/tabs/7/files/open?{query_prefix}{value}a");
        assert_eq!(
            parse(&request(Method::POST, &app_uri(&route))),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn distinguishes_wrong_methods_malformed_inputs_and_unknown_paths() {
        for route in [
            "/",
            "/tabs/7/view?layout=split&density=full",
            "/tabs/7/commits/abcdef0123456789abcdef0123456789abcdef01/view?layout=split&density=full",
            "/tabs/7/close",
            "/tabs/7/activate",
            "/tabs/7/refresh",
            "/history",
            "/history/42/open",
            "/settings?theme=dark",
            "/pending",
            "/ready",
            "/loads/9/next",
        ] {
            assert_eq!(
                parse(&request(Method::POST, &app_uri(route))),
                Err(StatusCode::METHOD_NOT_ALLOWED),
                "{route}"
            );
        }

        assert_eq!(
            parse(&request(Method::GET, &app_uri("/tabs/7/live-view"))),
            Err(StatusCode::METHOD_NOT_ALLOWED)
        );

        for route in [
            "/tabs/../view",
            "/tabs/0/view",
            "/tabs/7/view?layout=wide&density=full",
            "/tabs/7/view?layout=split",
            "/tabs/7/view?layout=split&density=full&layout=unified",
            "/tabs/7/commits/abc/view?layout=split&density=full",
            "/tabs/7/commits/zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz/view?layout=split&density=full",
            "/settings?theme=dark&theme=light",
            "/loads/0/next",
            "/loads/not-a-number/next",
            "/settings?unknown=value",
        ] {
            assert_eq!(
                parse(&request(Method::GET, &app_uri(route))),
                Err(StatusCode::BAD_REQUEST),
                "{route}"
            );
        }

        for uri in [
            format!("{}://wrong/tabs/7/view", protocol_config::PROTOCOL_SCHEME),
            format!(
                "{}://{}:1234/tabs/7/view?layout=split&density=full",
                protocol_config::PROTOCOL_SCHEME,
                protocol_config::APP_HOST
            ),
            app_uri("/tabs/7/unknown"),
            app_uri("/tabs/7/view/extra"),
            app_uri("/unknown"),
        ] {
            assert_eq!(
                parse(&request(Method::GET, &uri)),
                Err(StatusCode::NOT_FOUND),
                "{uri}"
            );
        }

        assert_eq!(
            parse(&request(Method::GET, &app_uri("/history/0/open"))),
            Err(StatusCode::BAD_REQUEST)
        );
    }
}
