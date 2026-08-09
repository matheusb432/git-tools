use dioxus::prelude::*;

pub(crate) mod application_layout;
pub(crate) mod application_navigation;
pub(crate) mod application_router;

use application_router::Route;

const FAVICON: Asset = asset!("/src/app/assets/app-icon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const DIFF_ISLAND_JS: Asset = asset!("/assets/generated/diff-island.js");
pub(crate) const DIFF_ISLAND_CSS: Asset = asset!("/assets/diff-island.css");

#[component]
pub(crate) fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Script { src: DIFF_ISLAND_JS }
        Router::<Route> {}
    }
}
