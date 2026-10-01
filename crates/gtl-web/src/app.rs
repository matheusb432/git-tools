use dioxus::prelude::*;

pub(crate) mod application_layout;
pub(crate) mod application_navigation;
pub(crate) mod application_router;
pub(crate) mod displayed_language;
mod projects_host;
pub(crate) mod user_settings;
mod window_chrome;
mod window_header;

use application_router::Route;

const FAVICON: Asset = asset!("/src/app/assets/app-icon.svg");
pub(crate) const VIEWER_CSS: Asset = asset!("/src/app/assets/styles/viewer.scss");
pub(crate) const DIFF_ROWS_CSS: Asset = asset!("/src/app/assets/styles/diff-rows.scss");

#[component]
pub(crate) fn App() -> Element {
    displayed_language::use_displayed_language_provider();
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: VIEWER_CSS }
        document::Link { rel: "stylesheet", href: DIFF_ROWS_CSS }
        Router::<Route> {}
    }
}
