use dioxus::prelude::*;

use crate::{
    app::application_layout::ApplicationLayout,
    views::{DiffHistoryView, DiffWorkspaceView, UserSettingsView},
};

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub(crate) enum Route {
    #[layout(ApplicationLayout)]
        #[route("/")]
        Workspace {},
        #[route("/history")]
        History {},
        #[route("/settings")]
        Settings {},
}

#[component]
fn Workspace() -> Element {
    rsx! {
        DiffWorkspaceView {}
    }
}

#[component]
fn History() -> Element {
    rsx! {
        DiffHistoryView {}
    }
}

#[component]
fn Settings() -> Element {
    rsx! {
        UserSettingsView {}
    }
}
