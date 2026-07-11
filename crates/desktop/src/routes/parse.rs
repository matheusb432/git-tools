use std::collections::HashMap;

use domain::viewer::{DiffDensity, DiffLayout, RenderHistoryId, RenderOptions, Theme, ViewerTabId};
use tauri::http::{Method, Request, StatusCode};

use crate::protocol_config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingChange {
    Layout(DiffLayout),
    Density(DiffDensity),
    Theme(Theme),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    Document,
    View {
        tab: ViewerTabId,
        options: RenderOptions,
    },
    Refresh {
        tab: ViewerTabId,
    },
    Close {
        tab: ViewerTabId,
    },
    Activate {
        tab: ViewerTabId,
    },
    History,
    OpenHistory {
        render: RenderHistoryId,
    },
    Settings(SettingChange),
    Pending,
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
        ["tabs", _, "refresh"] => RouteShape::Refresh,
        ["tabs", _, "close"] => RouteShape::Close,
        ["tabs", _, "activate"] => RouteShape::Activate,
        ["history"] => RouteShape::History,
        ["history", _, "open"] => RouteShape::OpenHistory,
        ["settings"] => RouteShape::Settings,
        ["pending"] => RouteShape::Pending,
        _ => return Err(StatusCode::NOT_FOUND),
    };
    if request.method() != Method::GET {
        return Err(StatusCode::METHOD_NOT_ALLOWED);
    }

    match shape {
        RouteShape::Document => {
            reject_query(uri.query())?;
            Ok(Route::Document)
        }
        RouteShape::View => {
            let tab = parse_tab_id(segments[1])?;
            let query = parse_query(uri.query())?;
            if query.len() != 2 {
                return Err(StatusCode::BAD_REQUEST);
            }
            let layout = query.get("layout").ok_or(StatusCode::BAD_REQUEST)?;
            let density = query.get("density").ok_or(StatusCode::BAD_REQUEST)?;
            let options = RenderOptions::try_from((*layout, *density))
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            Ok(Route::View { tab, options })
        }
        RouteShape::Refresh => tab_route(uri.query(), segments[1], |tab| Route::Refresh { tab }),
        RouteShape::Close => tab_route(uri.query(), segments[1], |tab| Route::Close { tab }),
        RouteShape::Activate => tab_route(uri.query(), segments[1], |tab| Route::Activate { tab }),
        RouteShape::History => {
            reject_query(uri.query())?;
            Ok(Route::History)
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
    }
}

#[derive(Debug, Clone, Copy)]
enum RouteShape {
    Document,
    View,
    Refresh,
    Close,
    Activate,
    History,
    OpenHistory,
    Settings,
    Pending,
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
    use domain::viewer::{DiffDensity, DiffLayout, RenderHistoryId, RenderOptions, ViewerTabId};
    use tauri::http::{Method, Request, StatusCode};

    use super::{Route, parse};
    use crate::protocol_config;

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
        for (uri, method, expected) in [
            (app_uri("/"), Method::GET, Route::Document),
            (
                app_uri("/tabs/7/view?layout=split&density=full"),
                Method::GET,
                Route::View {
                    tab,
                    options: RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
                },
            ),
            (
                app_uri("/tabs/7/refresh"),
                Method::GET,
                Route::Refresh { tab },
            ),
            (app_uri("/history"), Method::GET, Route::History),
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
    fn distinguishes_wrong_methods_malformed_inputs_and_unknown_paths() {
        for route in [
            "/",
            "/tabs/7/view?layout=split&density=full",
            "/tabs/7/close",
            "/tabs/7/activate",
            "/tabs/7/refresh",
            "/history",
            "/history/42/open",
            "/settings?theme=dark",
            "/pending",
        ] {
            assert_eq!(
                parse(&request(Method::POST, &app_uri(route))),
                Err(StatusCode::METHOD_NOT_ALLOWED),
                "{route}"
            );
        }

        for route in [
            "/tabs/../view",
            "/tabs/0/view",
            "/tabs/7/view?layout=wide&density=full",
            "/tabs/7/view?layout=split",
            "/tabs/7/view?layout=split&density=full&layout=unified",
            "/settings?theme=dark&theme=light",
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
