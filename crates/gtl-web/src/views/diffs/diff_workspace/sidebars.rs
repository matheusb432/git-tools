use dioxus::prelude::*;
use gtl_models::{
    settings::ViewerLanguage,
    viewer::{ViewerKeybindingAction, ViewerKeybindings, ViewerSidebarVisibility},
};
use lucide_dioxus::{PanelLeft, PanelRight};

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

    const fn action(self) -> ViewerKeybindingAction {
        match self {
            Self::Files => ViewerKeybindingAction::ToggleFilesSidebar,
            Self::Commits => ViewerKeybindingAction::ToggleCommitsSidebar,
        }
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
    artifact: bool,
) -> Element {
    let language = use_language();
    rsx! {
        div {
            class: "hidden items-center workspace:flex",
            role: "group",
            aria_label: t!(language, "sidebar-visibility"),
            for (sidebar, visible) in [(Sidebar::Files, visibility.files), (Sidebar::Commits, visibility.commits)] {
                SidebarButton {
                    key: "{sidebar.name()}",
                    sidebar,
                    visible,
                    keybindings,
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
    ontoggle: Option<EventHandler<Sidebar>>,
    artifact: bool,
) -> Element {
    let shortcut = keybindings
        .display_keys(sidebar.action())
        .map(|key| key.to_string())
        .collect::<Vec<_>>()
        .join("+");
    let label = sidebar.toggle_label(use_language());
    let title = format!("{label} ({shortcut})");
    rsx! {
        Button {
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Accent,
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
            span { class: "sidebar-icon", aria_hidden: "true",
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
}

#[cfg(feature = "desktop")]
pub(crate) fn use_sidebar_controls_provider() {
    let controls = use_sidebar_controls();
    use_context_provider(|| controls);
}

#[cfg(feature = "desktop")]
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
            artifact: false,
        }
    }
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy)]
pub(super) struct SidebarControls {
    pub(super) visibility: Memo<ViewerSidebarVisibility>,
    pub(super) toggle: Callback<Sidebar>,
}

#[cfg(feature = "desktop")]
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
