use dioxus::prelude::*;
use gtl_contracts::viewer::{
    SetViewerPreference, ViewerActiveState, ViewerActiveView, ViewerPreferences, ViewerShell,
    ViewerTabKind,
};
use lucide_dioxus::{FileDiff, LoaderCircle, RefreshCw};

use self::{
    commits_panel::CommitsPanel,
    display_controls::{DisplayControls, MobilePanelButton},
    files_panel::FilesPanel,
    keybar::Keybar,
    titlebar::ViewTitlebar,
};
use crate::{
    app::application_layout::{ViewerContext, ViewerShellLoad},
    entities::diffs::{ClientDiffSource, DiffViewerApi},
    shared::{
        bridge::ClientApiError,
        browser,
        ui::{
            AlertDialog, Button, ButtonSize, ButtonState, ButtonVariant, FloatingNotice,
            FloatingNoticeState, Popover, Skeleton,
        },
    },
    views::diffs::ClientDiffDocument,
};

mod commits_panel;
mod display_controls;
mod files_panel;
mod keybar;
mod titlebar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MobilePanel {
    Display,
    Files,
    Commits,
}

#[component]
pub(crate) fn DiffWorkspaceView() -> Element {
    let viewer = use_context::<ViewerContext>();
    let shell = viewer.read();

    use_effect(move || {
        browser::focus_element("workspace-heading".into());
    });

    rsx! {
        document::Title { "Viewer - git-tools" }
        main { class: "grid h-full min-h-0 grid-rows-[minmax(0,1fr)] overflow-hidden bg-bg",
            h1 { id: "workspace-heading", class: "sr-only", tabindex: "-1", "Diff viewer" }
            match shell {
                ViewerShellLoad::Loading => rsx! {
                    WorkspaceLoading {}
                },
                ViewerShellLoad::Error(error) => {
                    let message = error.message();
                    rsx! {
                        section { class: "grid h-full place-content-center px-5 text-center", role: "alert",
                            p { class: "font-semibold text-ink", "Viewer state is unavailable" }
                            p { class: "mt-1 max-w-md leading-5 text-ink-2", "{message}" }
                            Button {
                                class: "mx-auto mt-4",
                                variant: ButtonVariant::Outline,
                                onclick: move |_| viewer.reconnect(),
                                "Try again"
                            }
                        }
                    }
                }
                ViewerShellLoad::Ready(shell) => rsx! {
                    WorkspaceShell { shell }
                },
            }
        }
    }
}

#[component]
fn WorkspaceLoading() -> Element {
    rsx! {
        div {
            class: "grid h-full min-h-0 grid-rows-[2.75rem_3rem_minmax(0,1fr)]",
            role: "status",
            aria_label: "Loading viewer",
            div { class: "flex items-center gap-2 border-b border-line bg-surface px-3",
                Skeleton { class: "h-7 w-32" }
                Skeleton { class: "h-7 w-40" }
            }
            div { class: "flex items-center gap-2 border-b border-line bg-surface px-3",
                Skeleton { class: "h-7 w-52" }
                Skeleton { class: "ml-auto h-7 w-28" }
            }
            div { class: "grid min-h-0 grid-cols-1 gap-px bg-line xl:grid-cols-[15rem_minmax(0,1fr)_16rem]",
                Skeleton { class: "hidden h-full rounded-none xl:block" }
                Skeleton { class: "h-full rounded-none" }
                Skeleton { class: "hidden h-full rounded-none xl:block" }
            }
            span { class: "sr-only", "Loading viewer" }
        }
    }
}

#[component]
fn WorkspaceShell(shell: ViewerShell) -> Element {
    rsx! {
        section {
            id: "viewer-active-view",
            class: "flex h-full min-h-0 flex-col overflow-hidden",
            role: "tabpanel",
            aria_label: "Active diff",
            div { class: "min-h-0 flex-1 overflow-hidden",
                match shell.active {
                    ViewerActiveState::Empty => rsx! {
                        EmptyWorkspace {}
                    },
                    ViewerActiveState::Pending { .. } => rsx! {
                        PendingWorkspace {}
                    },
                    ViewerActiveState::Broken { code, message, .. } => rsx! {
                        WorkspaceFailure { title: format!("Render stopped ({code})"), message }
                    },
                    ViewerActiveState::Error { message, .. } => rsx! {
                        WorkspaceFailure { title: "Render failed".to_owned(), message }
                    },
                    ViewerActiveState::Ready { view } => {
                        let is_live = shell
                            .tabs
                            .iter()
                            .any(|tab| {
                                tab.id == view.identity.tab_id && tab.kind == ViewerTabKind::Live
                            });
                        rsx! {
                            ReadyWorkspace { view: *view, preferences: shell.preferences, is_live }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EmptyWorkspace() -> Element {
    rsx! {
        section { class: "grid h-full place-content-center px-5 text-center",
            span { class: "mx-auto text-acc", aria_hidden: "true",
                FileDiff { size: 22 }
            }
            h2 { class: "mt-3 font-semibold text-ink", "No diff is open" }
            p { class: "mt-1 max-w-md leading-5 text-ink-2",
                "Run a git-tools diff command to open a snapshot or live view."
            }
        }
    }
}

#[component]
fn PendingWorkspace() -> Element {
    rsx! {
        section {
            class: "grid h-full place-content-center px-5 text-center",
            role: "status",
            span {
                class: "mx-auto animate-spin text-acc motion-reduce:animate-none",
                aria_hidden: "true",
                LoaderCircle { size: 20 }
            }
            h2 { class: "mt-3 font-semibold text-ink", "Rendering diff" }
            p { class: "mt-1 text-ink-2", "The viewer will update when the render is ready." }
        }
    }
}

#[component]
fn WorkspaceFailure(title: String, message: String) -> Element {
    rsx! {
        section {
            class: "grid h-full place-content-center px-5 text-center",
            role: "alert",
            h2 { class: "font-semibold text-ink", "{title}" }
            p { class: "mt-1 max-w-md leading-5 text-ink-2", "{message}" }
        }
    }
}

#[component]
fn ReadyWorkspace(
    view: ViewerActiveView,
    preferences: ViewerPreferences,
    is_live: bool,
) -> Element {
    let viewer = use_context::<ViewerContext>();
    let mut action_error = use_signal(|| None::<ClientApiError>);
    let mut delete_open = use_signal(|| false);
    let mut delete_pending = use_signal(|| false);
    let mut delete_trigger_id = use_signal(|| "delete-live-view-desktop".to_owned());
    let mut mobile_panel = use_signal(|| None::<MobilePanel>);
    let mut file_filter = use_signal(String::new);
    let mut files_folded = use_signal(|| None::<bool>);
    let mut copy_context_enabled = use_signal(|| true);
    let mut flashing_file = use_signal(|| None::<String>);
    let tab_id = view.identity.tab_id;
    let identity = view.identity;

    let onpreference = move |preference: SetViewerPreference| {
        action_error.set(None);
        viewer.set_preference(preference);
    };
    let onrefresh = move |_| {
        action_error.set(None);
        viewer.refresh_tab(tab_id);
    };
    let onselect_commit = move |sha: String| {
        action_error.set(None);
        spawn(async move {
            match DiffViewerApi::select_commit(tab_id, sha).await {
                Ok(shell) => viewer.replace_shell(shell),
                Err(error) => action_error.set(Some(error)),
            }
        });
    };
    let onclear_commit = move |()| {
        action_error.set(None);
        spawn(async move {
            match DiffViewerApi::clear_commit_selection(tab_id).await {
                Ok(shell) => viewer.replace_shell(shell),
                Err(error) => action_error.set(Some(error)),
            }
        });
    };
    let onnavigate = move |anchor_id: String| {
        browser::scroll_to_file(anchor_id.clone());
        flashing_file.set(Some(anchor_id.clone()));
        spawn(async move {
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(1_200)).await;
            if flashing_file().as_deref() == Some(anchor_id.as_str()) {
                flashing_file.set(None);
            }
        });
    };
    let onopen = move |path: String| {
        action_error.set(None);
        spawn(async move {
            if let Err(error) = DiffViewerApi::open_diff_file(identity, path).await {
                action_error.set(Some(error));
            }
        });
    };
    let footer = view.footer.clone();

    rsx! {
        section { class: "grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden",
            div { class: "border-b border-line bg-surface px-3 py-2 text-ink-2",
                div { class: "hidden min-w-0 items-center justify-between gap-3 expanded:flex",
                    DisplayControls {
                        preferences,
                        pending: viewer.render_command_pending(),
                        is_live,
                        delete_trigger_id: "delete-live-view-desktop",
                        onpreference,
                        onrefresh,
                        ondelete: move |_| {
                            delete_trigger_id.set("delete-live-view-desktop".to_owned());
                            delete_open.set(true);
                        },
                    }
                }
                div { class: "flex items-center gap-2 expanded:hidden",
                    MobilePanelButton {
                        id: "mobile-display-trigger",
                        label: "Display",
                        icon: MobilePanel::Display,
                        onclick: move |_| mobile_panel.set(Some(MobilePanel::Display)),
                    }
                    MobilePanelButton {
                        id: "mobile-files-trigger",
                        label: "Files",
                        icon: MobilePanel::Files,
                        onclick: move |_| mobile_panel.set(Some(MobilePanel::Files)),
                    }
                    MobilePanelButton {
                        id: "mobile-commits-trigger",
                        label: "Commits",
                        icon: MobilePanel::Commits,
                        onclick: move |_| mobile_panel.set(Some(MobilePanel::Commits)),
                    }
                    Button {
                        class: "ml-auto",
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        aria_label: "Refresh diff",
                        title: "Refresh diff",
                        onclick: onrefresh,
                        if viewer.render_command_pending() {
                            span {
                                class: "animate-spin motion-reduce:animate-none",
                                aria_hidden: "true",
                                LoaderCircle { size: 14 }
                            }
                        } else {
                            span { aria_hidden: "true",
                                RefreshCw { size: 14 }
                            }
                        }
                    }
                }
                if viewer.render_command_pending() {
                    p { class: "sr-only", role: "status", "Applying the latest viewer update" }
                }
            }

            div { class: "grid min-h-0 grid-cols-[0_minmax(0,1fr)_0] grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden workspace:grid-cols-[220px_minmax(0,1fr)_210px] expanded:grid-cols-[262px_minmax(0,1fr)_252px] wide-screen:grid-cols-[320px_minmax(0,1fr)_304px]",
                ViewTitlebar {
                    view: view.clone(),
                    files_folded: files_folded().unwrap_or(false),
                    copy_context_enabled: copy_context_enabled(),
                    onfold: move |folded| files_folded.set(Some(folded)),
                    oncontext: move |enabled| copy_context_enabled.set(enabled),
                }
                aside {
                    class: "col-start-1 row-start-2 hidden min-h-0 overflow-hidden border-r border-line bg-surface workspace:block",
                    aria_label: "Changed files",
                    FilesPanel {
                        view: view.clone(),
                        filter: file_filter(),
                        onfilter: move |value| file_filter.set(value),
                        onnavigate,
                    }
                }
                ClientDiffDocument {
                    source: ClientDiffSource::Desktop,
                    view: view.clone(),
                    folded: files_folded(),
                    copy_context_enabled: copy_context_enabled(),
                    flashing_file: flashing_file(),
                    onopen,
                }
                aside {
                    class: "col-start-3 row-start-2 hidden min-h-0 overflow-hidden border-l border-line bg-surface workspace:block",
                    aria_label: "Commits",
                    CommitsPanel {
                        view: view.clone(),
                        onselect: onselect_commit,
                        onclear: onclear_commit,
                    }
                }
                Keybar { footer }
            }
        }

        if let Some(error) = action_error().or_else(|| viewer.render_command_error()) {
            FloatingNotice { state: FloatingNoticeState::Error, role: "alert", "{error.message()}" }
        }

        Popover {
            id: "mobile-display-panel",
            trigger_id: "mobile-display-trigger",
            open: mobile_panel() == Some(MobilePanel::Display),
            title: "Display controls",
            onclose: move |()| mobile_panel.set(None),
            div { class: "grid gap-3",
                DisplayControls {
                    preferences,
                    pending: viewer.render_command_pending(),
                    is_live,
                    delete_trigger_id: "delete-live-view-mobile",
                    onpreference,
                    onrefresh,
                    ondelete: move |_| {
                        mobile_panel.set(None);
                        delete_trigger_id.set("mobile-display-trigger".to_owned());
                        delete_open.set(true);
                    },
                }
                div { class: "grid grid-cols-2 gap-2",
                    Button {
                        variant: ButtonVariant::Outline,
                        onclick: move |_| files_folded.set(Some(!files_folded().unwrap_or(false))),
                        if files_folded().unwrap_or(false) {
                            "Expand all"
                        } else {
                            "Collapse all"
                        }
                    }
                    Button {
                        variant: if copy_context_enabled() { ButtonVariant::Pressed } else { ButtonVariant::Outline },
                        aria_pressed: copy_context_enabled().to_string(),
                        onclick: move |_| copy_context_enabled.set(!copy_context_enabled()),
                        "+ context"
                    }
                }
            }
        }
        Popover {
            id: "mobile-files-panel",
            trigger_id: "mobile-files-trigger",
            open: mobile_panel() == Some(MobilePanel::Files),
            title: "Changed files",
            onclose: move |()| mobile_panel.set(None),
            FilesPanel {
                view: view.clone(),
                filter: file_filter(),
                onfilter: move |value| file_filter.set(value),
                onnavigate,
            }
        }
        Popover {
            id: "mobile-commits-panel",
            trigger_id: "mobile-commits-trigger",
            open: mobile_panel() == Some(MobilePanel::Commits),
            title: "Commits",
            onclose: move |()| mobile_panel.set(None),
            CommitsPanel {
                view: view.clone(),
                onselect: onselect_commit,
                onclear: onclear_commit,
            }
        }
        AlertDialog {
            id: "delete-live-view-dialog",
            trigger_id: delete_trigger_id(),
            open: delete_open(),
            title: "Delete live view",
            description: "This removes the saved live view and closes its tab. Render history remains available.",
            confirm_label: "Delete live view",
            confirm_state: if delete_pending() { ButtonState::Loading } else { ButtonState::Enabled },
            cancel_disabled: delete_pending(),
            oncancel: move |()| {
                if !delete_pending() {
                    delete_open.set(false);
                }
            },
            onconfirm: move |()| {
                if delete_pending() {
                    return;
                }
                delete_pending.set(true);
                action_error.set(None);
                spawn(async move {
                    match DiffViewerApi::delete_live_tab(tab_id).await {
                        Ok(shell) => {
                            delete_open.set(false);
                            viewer.replace_shell(shell);
                        }
                        Err(error) => action_error.set(Some(error)),
                    }
                    delete_pending.set(false);
                });
            },
        }
    }
}

pub(super) const fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}
