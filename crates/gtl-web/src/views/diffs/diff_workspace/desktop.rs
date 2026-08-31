use dioxus::prelude::*;
use gtl_models::diffs::CommitId;
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::{
    CommitSelectionAction, OpenViewerDiffFile, SetViewerPreference, ViewerActiveState,
    ViewerActiveView, ViewerTabKind, ViewerTabRequest, make_commit_selection_action,
};
use lucide_dioxus::{FileDiff, RefreshCw};

use super::{
    DiffWorkspaceDocument, MobilePanel,
    commits_panel::WorkspaceCommitsPanel,
    display_controls::{DisplayControls, MobilePanelButton},
    files_panel::FilesPanel,
    titlebar::{ViewActions, ViewActionsLayout},
};
use crate::{
    app::application_layout::{ViewerContext, ViewerShellLoad},
    entities::diffs::{use_viewer_commit_pages, viewer_server},
    shared::{
        browser,
        ui::{
            AlertDialog, Button, ButtonSize, ButtonState, ButtonVariant, Popover, Skeleton,
            use_toast,
        },
    },
    views::diffs::ClientDiffDocument,
};

#[component]
pub(crate) fn DiffWorkspaceView() -> Element {
    let viewer = use_context::<ViewerContext>();
    let shell = viewer.shell();

    use_effect(move || {
        browser::focus_element("workspace-heading".into());
    });

    rsx! {
        document::Title { "Viewer - git-tools" }
        main { class: "grid h-full min-h-0 grid-rows-[minmax(0,1fr)] overflow-hidden bg-bg",
            h1 { id: "workspace-heading", class: "sr-only", tabindex: "-1", "Diff viewer" }
            match &*shell.read() {
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
                ViewerShellLoad::Ready(_) => rsx! {
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
fn WorkspaceShell(shell: ReadSignal<ViewerShellLoad>) -> Element {
    let view = use_hook(move || shell.map(ready_active_view));
    let shell_state = shell.read();
    let ViewerShellLoad::Ready(shell_state) = &*shell_state else {
        return rsx! {};
    };
    rsx! {
        section {
            id: "viewer-active-view",
            class: "flex h-full min-h-0 flex-col overflow-hidden",
            role: "tabpanel",
            aria_label: "Active diff",
            div { class: "min-h-0 flex-1 overflow-hidden",
                match &shell_state.active {
                    ViewerActiveState::Empty => rsx! {
                        EmptyWorkspace {}
                    },
                    ViewerActiveState::Pending { .. } => rsx! {},
                    ViewerActiveState::Broken { code, message, .. } => rsx! {
                        WorkspaceFailure {
                            title: format!("Render stopped ({})", code.as_str()),
                            message: message.clone(),
                        }
                    },
                    ViewerActiveState::Error { message, .. } => rsx! {
                        WorkspaceFailure { title: "Render failed".to_owned(), message: message.clone() }
                    },
                    ViewerActiveState::Ready { .. } => rsx! {
                        ReadyWorkspace { view, shell }
                    },
                }
            }
        }
    }
}

// The mapped signal is created only for the parent branch that owns a ready active view.
#[allow(clippy::unreachable)]
fn ready_active_view(shell: &ViewerShellLoad) -> &ViewerActiveView {
    let ViewerShellLoad::Ready(shell) = shell else {
        unreachable!("the ready workspace is mounted only for a ready shell");
    };
    let ViewerActiveState::Ready { view } = &shell.active else {
        unreachable!("the ready workspace is mounted only for a ready active view");
    };
    view
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
    view: ReadSignal<ViewerActiveView>,
    shell: ReadSignal<ViewerShellLoad>,
) -> Element {
    let viewer = use_context::<ViewerContext>();
    let toast = use_toast();
    let mut delete_open = use_signal(|| false);
    let mut delete_pending = use_signal(|| false);
    let mut delete_trigger_id = use_signal(|| "delete-live-view-desktop".to_owned());
    let mut mobile_panel = use_signal(|| None::<MobilePanel>);
    let file_filter = use_signal(String::new);
    let files_folded = use_signal(|| None::<bool>);
    let copy_context_enabled = use_signal(|| true);
    let mut flashing_file = use_signal(|| None::<String>);
    let mut find_open = use_signal(|| false);
    let commit_pages = use_viewer_commit_pages(view);
    let _workspace = super::use_diff_workspace_context(
        view,
        commit_pages.commits(),
        super::DiffWorkspaceSignals {
            file_filter,
            files_folded,
            copy_context_enabled,
            flashing_file,
            find_open,
        },
        true,
    );
    let (tab_id, identity) = view.with(|view| (view.identity.tab_id, view.identity));
    let ready_shell = shell.with(|shell| {
        let ViewerShellLoad::Ready(shell) = shell else {
            return None;
        };
        Some((
            shell.preferences,
            shell
                .tabs
                .iter()
                .any(|tab| tab.id == tab_id && tab.kind == ViewerTabKind::Live),
        ))
    });
    let commits_loading = commit_pages.is_loading();
    let commits_error = commit_pages.error().map(|error| error.message().to_owned());
    let commits_has_more = commit_pages.has_more();
    let onload_commits = use_callback(move |()| commit_pages.load_next());

    let onpreference = move |preference: SetViewerPreference| viewer.set_preference(preference);
    let onrefresh = move |_| viewer.refresh_tab(tab_id);

    // TODO: remove unit from param
    let onclear_commit = use_callback(move |()| {
        spawn(async move {
            match viewer_server::clear_commit_selection(ViewerTabRequest { tab_id }).await {
                Ok(shell) => viewer.replace_shell(shell),
                Err(error) => toast.error(error.message()),
            }
        });
    });

    let onselect_commit = use_callback(move |id: CommitId| {
        let commit_selection = view.peek().commit_selection.clone();
        match make_commit_selection_action(&commit_selection, tab_id, id) {
            CommitSelectionAction::FetchCommit(request) => {
                spawn(async move {
                    match viewer_server::select_commit(request).await {
                        Ok(shell) => viewer.replace_shell(shell),
                        Err(error) => toast.error(error.message()),
                    }
                });
            }
            CommitSelectionAction::UnselectCommit => onclear_commit.call(()),
            CommitSelectionAction::NoAction => {}
        }
    });

    let onnavigate = move |anchor_id: String| {
        browser::scroll_to_file(&anchor_id);
        flashing_file.set(Some(anchor_id.clone()));
        spawn(async move {
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(1_200)).await;
            if flashing_file().as_deref() == Some(anchor_id.as_str()) {
                flashing_file.set(None);
            }
        });
    };
    let onopen = move |file: gtl_wire::viewer::ViewerDiffFileId| {
        spawn(async move {
            if let Err(error) =
                viewer_server::open_diff_file(OpenViewerDiffFile { identity, file }).await
            {
                toast.error(error.message());
            }
        });
    };
    let Some((preferences, is_live)) = ready_shell else {
        return rsx! {};
    };
    rsx! {
        section {
            class: "grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden",
            onkeydown: move |event: KeyboardEvent| {
                if is_diff_find_shortcut(&event) {
                    event.prevent_default();
                    find_open.set(true);
                    browser::focus_element("viewer-diff-find-input".to_owned());
                }
            },
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
                    MobileRefreshButton {
                        pending: viewer.render_command_pending(),
                        onrefresh,
                    }
                }
                if viewer.render_command_pending() {
                    p { class: "sr-only", role: "status", "Applying the latest viewer update" }
                }
            }

            DiffWorkspaceDocument {
                diff_document: rsx! {
                    ClientDiffDocument { onopen }
                },
                onnavigate,
                onselect_commit,
                onclear_commit,
                commits_loading,
                commits_error: commits_error.clone(),
                commits_has_more,
                onload_commits,
            }
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
                ViewActions { layout: ViewActionsLayout::Panel }
            }
        }
        Popover {
            id: "mobile-files-panel",
            trigger_id: "mobile-files-trigger",
            open: mobile_panel() == Some(MobilePanel::Files),
            title: "Changed files",
            onclose: move |()| mobile_panel.set(None),
            FilesPanel { onnavigate }
        }
        Popover {
            // TODO: organize this more intuitively. not obvious that this is where the mobile view is.
            id: "mobile-commits-panel",
            trigger_id: "mobile-commits-trigger",
            open: mobile_panel() == Some(MobilePanel::Commits),
            title: "Commits",
            onclose: move |()| mobile_panel.set(None),
            WorkspaceCommitsPanel {
                onselect: onselect_commit,
                onclear: onclear_commit,
                loading: commits_loading,
                load_error: commits_error,
                has_more: commits_has_more,
                onloadmore: onload_commits,
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
                spawn(async move {
                    match viewer_server::delete_live_tab(ViewerTabRequest { tab_id }).await {
                        Ok(shell) => {
                            delete_open.set(false);
                            viewer.replace_shell(shell);
                        }
                        Err(error) => toast.error(error.message()),
                    }
                    delete_pending.set(false);
                });
            },
        }
    }
}

fn is_diff_find_shortcut(event: &KeyboardEvent) -> bool {
    let modifiers = event.modifiers();
    (modifiers.ctrl() || modifiers.meta())
        && matches!(event.key(), Key::Character(value) if value.eq_ignore_ascii_case("f"))
}

#[component]
fn MobileRefreshButton(pending: bool, onrefresh: EventHandler<MouseEvent>) -> Element {
    rsx! {
        Button {
            class: "ml-auto",
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            state: if pending { ButtonState::Loading } else { ButtonState::Enabled },
            aria_label: "Refresh diff",
            title: "Refresh diff",
            "data-testid": test_ids::LIVE_VIEW_REFRESH.value(),
            onclick: onrefresh,
            if !pending {
                span { aria_hidden: "true",
                    RefreshCw { size: 14 }
                }
            }
        }
    }
}
