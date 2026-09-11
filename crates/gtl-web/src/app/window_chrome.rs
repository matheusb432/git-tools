use dioxus::prelude::*;
use gtl_wire::window::{WindowAction, WindowState};
use lucide_dioxus::{Copy, Minus, X};

#[component]
pub(super) fn WindowTitleBar(
    state: Option<WindowState>,
    onaction: EventHandler<WindowAction>,
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
    rsx! {
        div {
            class: "viewer-window-drag-region",
            "data-tauri-drag-region": "",
            title: "Drag to move window",
        }
    }
}

#[component]
fn WindowControls(maximized: bool, onaction: EventHandler<WindowAction>) -> Element {
    rsx! {
        div {
            class: "viewer-window-controls",
            role: "group",
            aria_label: "Window controls",
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
    const fn label(self) -> &'static str {
        match self {
            Self::Minimize => "Minimize window",
            Self::Maximize => "Maximize window",
            Self::Restore => "Restore window",
            Self::Close => "Close window",
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
    rsx! {
        button {
            class: "viewer-window-button",
            r#type: "button",
            "data-close": (control == WindowControl::Close).to_string(),
            aria_label: control.label(),
            title: control.label(),
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
