use dioxus::prelude::*;
use gtl_models::{
    settings::ViewerLanguage,
    viewer::{ViewerKeybindingAction, ViewerKeybindings, ViewerSidebarVisibility},
};

use crate::shared::{
    i18n::{t, use_language},
    ui::{Button, ButtonSize, ButtonVariant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sidebar {
    Files,
    Commits,
}

impl Sidebar {
    fn drag_toggles(self, visible: bool, distance: f64) -> bool {
        let outward = match self {
            Self::Files => -distance,
            Self::Commits => distance,
        };
        if visible {
            outward >= 40.0
        } else {
            outward <= -40.0
        }
    }
    const fn name(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Commits => "commits",
        }
    }

    fn toggle_label(self, language: ViewerLanguage) -> String {
        match self {
            Self::Files => t!(language, "sidebar-toggle-files"),
            Self::Commits => t!(language, "sidebar-toggle-commits"),
        }
    }

    /// Returns the panel fill and divider paths inside the 24px sidebar icon frame.
    const fn icon_paths(self) -> (&'static str, &'static str) {
        match self {
            Self::Files => ("M5 3h4v18H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z", "M9 3v18"),
            Self::Commits => ("M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4z", "M15 3v18"),
        }
    }

    const fn action(self) -> ViewerKeybindingAction {
        match self {
            Self::Files => ViewerKeybindingAction::ToggleFilesSidebar,
            Self::Commits => ViewerKeybindingAction::ToggleCommitsSidebar,
        }
    }
}

#[component]
pub(super) fn SidebarEdge(
    sidebar: Sidebar,
    visible: bool,
    ontoggle: EventHandler<Sidebar>,
) -> Element {
    let mut gesture = use_signal(|| None::<(i32, f64)>);
    let mut ready = use_signal(|| false);
    let mut suppress_click = use_signal(|| false);
    let label = sidebar.toggle_label(use_language());
    rsx! {
        button {
            r#type: "button",
            class: "sidebar-edge",
            "data-sidebar-edge": sidebar.name(),
            "data-ready": ready().to_string(),
            "data-visible": visible.to_string(),
            aria_label: label,
            aria_expanded: visible.to_string(),
            onpointerdown: move |event: PointerEvent| {
                if event.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary)
                {
                    return;
                }
                suppress_click.set(false);
                if capture_pointer(&event) {
                    gesture.set(Some((event.pointer_id(), event.client_coordinates().x)));
                    ready.set(false);
                }
            },
            onpointermove: move |event: PointerEvent| {
                if let Some((pointer, start)) = gesture()
                    && pointer == event.pointer_id() {
                    ready
                        .set(
                            sidebar.drag_toggles(visible, event.client_coordinates().x - start),
                        );
                }
            },
            onpointerup: move |event: PointerEvent| {
                if let Some((pointer, start)) = gesture()
                    && pointer == event.pointer_id() {
                    gesture.set(None);
                    ready.set(false);
                    suppress_click.set(true);
                    if sidebar.drag_toggles(visible, event.client_coordinates().x - start) {
                        ontoggle.call(sidebar);
                    }
                }
            },
            onpointercancel: move |_| {
                gesture.set(None);
                ready.set(false);
            },
            onlostpointercapture: move |_| {
                gesture.set(None);
                ready.set(false);
            },
            onclick: move |_| {
                if !suppress_click() {
                    ontoggle.call(sidebar);
                }
            },
            onkeydown: move |event: KeyboardEvent| {
                suppress_click.set(false);
                if event.key() == Key::Escape {
                    gesture.set(None);
                    ready.set(false);
                    suppress_click.set(true);
                }
            },
        }
    }
}

fn capture_pointer(event: &PointerEvent) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast as _;
        event
            .data()
            .downcast::<web_sys::PointerEvent>()
            .and_then(|event| event.target())
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|element| element.set_pointer_capture(event.pointer_id()).is_ok())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = event;
        false
    }
}

#[component]
pub(super) fn SidebarPanel(sidebar: Sidebar, visible: bool, children: Element) -> Element {
    let language = use_language();
    let (class, label) = match sidebar {
        Sidebar::Files => (
            "diff-workspace-files-panel",
            t!(language, "workspace-changed-files"),
        ),
        Sidebar::Commits => (
            "diff-workspace-commits-panel",
            t!(language, "workspace-commits"),
        ),
    };
    rsx! {
        aside {
            class: "diff-workspace-panel diff-workspace-sidebar min-h-0 {class}",
            "data-sidebar-panel": sidebar.name(),
            "inert": (!visible).then_some(""),
            aria_label: label,
            aria_hidden: (!visible).to_string(),
            div { class: "diff-workspace-sidebar-content", {children} }
        }
    }
}

#[component]
pub(super) fn SidebarButtons(
    visibility: ViewerSidebarVisibility,
    keybindings: ViewerKeybindings,
    ontoggle: Option<EventHandler<Sidebar>>,
) -> Element {
    let language = use_language();
    rsx! {
        div {
            class: "hidden items-center gap-0.5 workspace:flex",
            role: "group",
            aria_label: t!(language, "sidebar-visibility"),
            for (sidebar, visible) in [(Sidebar::Files, visibility.files), (Sidebar::Commits, visibility.commits)] {
                SidebarButton {
                    key: "{sidebar.name()}",
                    sidebar,
                    visible,
                    keybindings,
                    ontoggle,
                }
            }
        }
    }
}

#[component]
fn SidebarButton(
    sidebar: Sidebar,
    visible: bool,
    keybindings: ViewerKeybindings,
    ontoggle: Option<EventHandler<Sidebar>>,
) -> Element {
    let shortcut = keybindings
        .display_keys(sidebar.action())
        .map(|key| key.to_string())
        .collect::<Vec<_>>()
        .join("+");
    let label = sidebar.toggle_label(use_language());
    let title = format!("{label} ({shortcut})");
    let (panel, divider) = sidebar.icon_paths();
    rsx! {
        Button {
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            class: "sidebar-toggle",
            id: format!("{}-sidebar-toggle", sidebar.name()),
            aria_label: label,
            title,
            aria_pressed: visible.to_string(),
            "data-sidebar-toggle": sidebar.name(),

            onclick: move |_| {
                if let Some(ontoggle) = ontoggle {
                    ontoggle.call(sidebar);
                }
            },
            svg {
                class: "sidebar-icon",
                "aria-hidden": "true",
                width: "16",
                height: "16",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path {
                    class: "sidebar-icon-panel",
                    d: panel,
                    fill: "currentColor",
                    stroke: "none",
                }
                rect {
                    width: "18",
                    height: "18",
                    x: "3",
                    y: "3",
                    rx: "2",
                }
                path { d: divider }
            }
        }
    }
}

pub(crate) fn use_sidebar_controls_provider() {
    let controls = use_sidebar_controls();
    use_context_provider(|| controls);
}

#[component]
pub(crate) fn WorkspaceSidebarButtons() -> Element {
    use crate::app::application_layout::{ViewerContext, ViewerShellLoad};

    let controls = use_context::<SidebarControls>();
    let viewer = use_context::<ViewerContext>();
    let keybindings = viewer.shell().with(|shell| match shell {
        ViewerShellLoad::Ready(shell) => shell.preferences.keybindings,
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => ViewerKeybindings::default(),
    });
    rsx! {
        SidebarButtons {
            visibility: (controls.visibility)(),
            keybindings,
            ontoggle: controls.toggle,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SidebarControls {
    pub(super) visibility: Memo<ViewerSidebarVisibility>,
    pub(super) toggle: Callback<Sidebar>,
}

fn use_sidebar_controls() -> SidebarControls {
    use gtl_wire::viewer::{EditSettingsRequest, FieldUpdate};

    use crate::{
        app::application_layout::{ViewerContext, ViewerShellLoad},
        entities::diffs::viewer_server,
        shared::ui::use_toast,
    };

    let viewer = use_context::<ViewerContext>();
    let shell = viewer.shell();
    let visibility = use_memo(move || match &*shell.read() {
        ViewerShellLoad::Ready(shell) => shell.preferences.sidebars,
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => ViewerSidebarVisibility::default(),
    });
    let toast = use_toast();
    let mut pending = use_signal(|| false);
    let mut save = use_action(move |request: EditSettingsRequest| async move {
        let focus = if request.files_sidebar_visible == FieldUpdate::Update(false) {
            Some("files-sidebar-toggle")
        } else if request.commits_sidebar_visible == FieldUpdate::Update(false) {
            Some("commits-sidebar-toggle")
        } else {
            None
        };
        let result = async {
            viewer_server::edit_settings(request).await?;
            viewer_server::get_shell().await
        }
        .await;
        pending.set(false);
        let shell = match result {
            Ok(shell) => shell,
            Err(error) => {
                toast.client_error(&error);
                return Ok::<(), std::convert::Infallible>(());
            }
        };
        viewer.replace_shell(shell);
        if let Some(id) = focus {
            crate::shared::browser::focus_element(id.to_owned());
        }
        Ok::<(), std::convert::Infallible>(())
    });
    let toggle = use_callback(move |sidebar: Sidebar| {
        if *pending.peek() {
            return;
        }
        let current = *visibility.peek();
        let mut request = EditSettingsRequest::default();
        match sidebar {
            Sidebar::Files => request.files_sidebar_visible = FieldUpdate::Update(!current.files),
            Sidebar::Commits => {
                request.commits_sidebar_visible = FieldUpdate::Update(!current.commits);
            }
        }
        pending.set(true);
        save.call(request);
    });
    SidebarControls { visibility, toggle }
}

#[cfg(test)]
mod tests {
    use super::Sidebar;

    #[test]
    fn panel_drags_require_a_deliberate_movement_toward_the_target_edge() {
        for (sidebar, outward) in [(Sidebar::Files, -40.0), (Sidebar::Commits, 40.0)] {
            assert!(sidebar.drag_toggles(true, outward));
            assert!(!sidebar.drag_toggles(true, outward / 2.0));
            assert!(!sidebar.drag_toggles(true, -outward));
            assert!(sidebar.drag_toggles(false, -outward));
            assert!(!sidebar.drag_toggles(false, outward));
        }
    }
}
