use dioxus::prelude::*;

use super::{application_layout::ViewerContext, application_router::Route};

#[component]
pub(super) fn ProjectsHost() -> Element {
    let viewer = use_context::<ViewerContext>();
    let route = use_route::<Route>();
    let active = use_memo(use_reactive((&route,), |(route,)| {
        matches!(route, Route::Projects {})
    }));
    let mut visited = use_signal(&*active);
    use_effect(move || {
        if active() && !*visited.peek() {
            visited.set(true);
        }
    });
    let instance = viewer.server_instance_id();

    rsx! {
        div {
            class: "h-full min-h-0",
            hidden: !active(),
            "inert": (!active()).then_some(""),
            if visited() || active() {
                crate::views::projects::ProjectsView { key: "{instance:?}", route_active: active }
            }
        }
    }
}
