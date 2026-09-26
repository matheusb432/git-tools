use dioxus::prelude::*;
use gtl_wire::window::WindowAction;

use super::{
    application_layout::ViewerContext, application_navigation::ApplicationNavigation,
    application_router::Route, window_chrome::WindowTitleBar,
};
use crate::{
    shared::{browser, ui::use_toast},
    views::{
        diffs::diff_workspace::sidebars::WorkspaceSidebarButtons,
        viewer_settings_button::ViewerSettingsButton,
    },
};

#[component]
pub(super) fn WindowHeader() -> Element {
    let viewer = use_context::<ViewerContext>();
    let navigator = use_navigator();
    let route = use_route::<Route>();
    let mut revision = use_signal(|| 0_u64);
    browser::use_window_resize(move || revision += 1);
    let window = use_resource(move || async move {
        let _ = revision();
        gtl_client::window::state().await
    });
    let toast = use_toast();
    let mut action = use_action(move |request: WindowAction| async move {
        match gtl_client::window::perform(request).await {
            Ok(()) => revision += 1,
            Err(error) => toast.client_error(&error),
        }
        Ok::<(), std::convert::Infallible>(())
    });
    let state = window
        .read()
        .as_ref()
        .and_then(|state| state.as_ref().ok())
        .copied();
    rsx! {
        WindowTitleBar {
            state,
            onaction: move |request| {
                action.call(request);
            },
            actions: rsx! {
                div {
                    class: "viewer-window-actions",
                    "inert": (!viewer.actions_enabled()).then_some(""),
                    ViewerSettingsButton {
                        onsettings: move |()| {
                            navigator.push(Route::Settings {});
                        },
                    }
                    if matches!(route, Route::Diff { .. } | Route::CurrentDiff {}) {
                        WorkspaceSidebarButtons {}
                    }
                }
            },
            div {
                class: "min-w-0 flex-1",
                "inert": (!viewer.actions_enabled()).then_some(""),
                ApplicationNavigation {}
            }
        }
    }
}
