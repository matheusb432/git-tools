use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use gtl_wire::window::{WindowAction, WindowState};
use lucide_dioxus::{Copy, Minus, X};

use crate::shared::i18n::{t, use_language};

#[component]
pub(super) fn WindowTitleBar(
    state: Option<WindowState>,
    onaction: EventHandler<WindowAction>,
    actions: Element,
    children: Element,
) -> Element {
    let custom_titlebar = state.is_some_and(|state| state.custom_titlebar);
    rsx! {
        header {
            class: "viewer-window-header",
            "data-tauri-drag-region": custom_titlebar.then_some("deep"),
            {children}
            if custom_titlebar {
                WindowDragRegion {}
            }
            WindowDragExcluded { {actions} }
            if custom_titlebar {
                WindowControls {
                    maximized: state.is_some_and(|state| state.maximized),
                    onaction,
                }
            }
        }
    }
}

#[component]
pub(super) fn WindowDragExcluded(children: Element) -> Element {
    rsx! {
        div { class: "contents", "data-tauri-drag-region": "false", {children} }
    }
}

#[component]
fn WindowDragRegion() -> Element {
    let language = use_language();
    rsx! {
        div {
            class: "viewer-window-drag-region",
            "data-tauri-drag-region": "",
            title: t!(language, "window-drag-region"),
        }
    }
}

#[component]
fn WindowControls(maximized: bool, onaction: EventHandler<WindowAction>) -> Element {
    let language = use_language();
    rsx! {
        div {
            class: "viewer-window-controls",
            role: "group",
            aria_label: t!(language, "window-controls"),
            WindowControlButton { control: WindowControl::Minimize, onaction }
            WindowControlButton {
                control: if maximized { WindowControl::Restore } else { WindowControl::Maximize },
                onaction,
            }
            WindowControlButton { control: WindowControl::Close, onaction }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WindowControl {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl WindowControl {
    fn label(self, language: ViewerLanguage) -> String {
        match self {
            Self::Minimize => t!(language, "window-minimize"),
            Self::Maximize => t!(language, "window-maximize"),
            Self::Restore => t!(language, "window-restore"),
            Self::Close => t!(language, "window-close"),
        }
    }

    const fn action(self) -> WindowAction {
        match self {
            Self::Minimize => WindowAction::Minimize,
            Self::Maximize | Self::Restore => WindowAction::ToggleMaximize,
            Self::Close => WindowAction::Close,
        }
    }
}

#[component]
fn WindowControlButton(control: WindowControl, onaction: EventHandler<WindowAction>) -> Element {
    let label = control.label(use_language());
    rsx! {
        button {
            class: "viewer-window-button",
            r#type: "button",
            "data-close": (control == WindowControl::Close).to_string(),
            aria_label: label.clone(),
            title: label,
            onclick: move |_| onaction.call(control.action()),
            span { class: "inline-flex", aria_hidden: "true",
                match control {
                    WindowControl::Minimize => rsx! {
                        Minus { size: 14 }
                    },
                    WindowControl::Maximize => rsx! {
                        span { class: "size-2.5 border border-current" }
                    },
                    WindowControl::Restore => rsx! {
                        Copy { size: 12 }
                    },
                    WindowControl::Close => rsx! {
                        X { size: 15 }
                    },
                }
            }
        }
    }
}
