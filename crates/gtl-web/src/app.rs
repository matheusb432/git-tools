use dioxus::prelude::*;

pub(crate) mod application_layout;
pub(crate) mod application_navigation;
pub(crate) mod application_router;
mod projects_host;
mod window_chrome;
mod window_header;

use application_router::Route;

const FAVICON: Asset = asset!("/src/app/assets/app-icon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[component]
pub(crate) fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        Router::<Route> {}
    }
}
