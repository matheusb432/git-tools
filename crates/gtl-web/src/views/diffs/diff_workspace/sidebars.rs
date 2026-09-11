use dioxus::prelude::*;
use gtl_models::viewer::{ViewerKeybindingAction, ViewerKeybindings, ViewerSidebarVisibility};
use lucide_dioxus::{PanelLeft, PanelRight};

use crate::shared::ui::{Button, ButtonSize, ButtonState, ButtonVariant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sidebar {
    Files,
    Commits,
}

impl Sidebar {
    const fn name(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Commits => "commits",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Commits => "Commits",
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
pub(super) fn SidebarPanel(sidebar: Sidebar, visible: bool, children: Element) -> Element {
    let (class, label) = match sidebar {
        Sidebar::Files => ("diff-workspace-files-panel", "Changed files"),
        Sidebar::Commits => ("diff-workspace-commits-panel", "Commits"),
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
    pending: bool,
    ontoggle: Option<EventHandler<Sidebar>>,
    artifact: bool,
) -> Element {
    let state = if pending {
        ButtonState::Disabled
    } else {
        ButtonState::Enabled
    };
    rsx! {
        div {
            class: "hidden items-center gap-1 workspace:flex",
            role: "group",
            aria_label: "Sidebar visibility",
            for (sidebar, visible) in [(Sidebar::Files, visibility.files), (Sidebar::Commits, visibility.commits)] {
                SidebarButton {
                    key: "{sidebar.name()}",
                    sidebar,
                    visible,
                    keybindings,
                    state,
                    ontoggle,
                    artifact,
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
    state: ButtonState,
    ontoggle: Option<EventHandler<Sidebar>>,
    artifact: bool,
) -> Element {
    let shortcut = keybindings
        .display_keys(sidebar.action())
        .map(|key| key.to_string())
        .collect::<Vec<_>>()
        .join("+");
    let label = format!("Toggle {} sidebar", sidebar.label());
    let title = format!("{label} ({shortcut})");
    rsx! {
        Button {
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Toggle,
            state,
            id: (!artifact).then(|| format!("{}-sidebar-toggle", sidebar.name())),
            aria_label: label,
            title,
            aria_pressed: visible.to_string(),
            "data-sidebar-toggle": sidebar.name(),
            "data-gtl-action": artifact.then(|| format!("toggle-{}-sidebar", sidebar.name())),
            onclick: move |_| {
                if let Some(ontoggle) = ontoggle {
                    ontoggle.call(sidebar);
                }
            },
            match sidebar {
                Sidebar::Files => rsx! {
                    PanelLeft { size: 16 }
                },
                Sidebar::Commits => rsx! {
                    PanelRight { size: 16 }
                },
            }
        }
    }
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy)]
pub(super) struct SidebarControls {
    pub(super) visibility: Memo<ViewerSidebarVisibility>,
    pub(super) pending: ReadSignal<bool>,
    pub(super) toggle: Callback<Sidebar>,
}

#[cfg(feature = "desktop")]
pub(super) fn use_sidebar_controls() -> SidebarControls {
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
                toast.error(error.message());
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
    SidebarControls {
        visibility,
        pending: pending.into(),
        toggle,
    }
}
