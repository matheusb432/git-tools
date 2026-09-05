use dioxus::prelude::*;
use gtl_models::{
    diffs::CommitId,
    viewer::{ViewerKeybindingAction, ViewerKeybindings},
};
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::{
    CommitSelectionAction, OpenViewerDiffFile, ViewerActiveState, ViewerActiveView, ViewerTabKind,
    ViewerTabRequest, make_commit_selection_action,
};
use lucide_dioxus::{Ellipsis, FileDiff, RefreshCw, Trash2};

use super::{
    DiffWorkspaceDocument, MobilePanel, WorkspaceMobileNavigation,
    commits_panel::WorkspaceCommitsPanel, files_panel::FilesPanel,
};
use crate::{
    app::application_layout::{ViewerContext, ViewerShellLoad},
    entities::diffs::{use_viewer_commit_pages, viewer_server},
    shared::{
        browser,
        ui::{
            AlertDialog, Button, ButtonSize, ButtonState, ButtonVariant, IconPopover,
            IconPopoverPlacement, MENU_ACTION_HOST_CLASSES, MenuActionContent, PageNotice, Popover,
            Skeleton, use_toast,
        },
    },
    views::diffs::{ClientDiffDocument, search_keybindings::native_keyboard_event_matches},
};

const LIVE_VIEW_ACTIONS_POPOVER_ID: &str = "live-view-actions";
const DELETE_LIVE_VIEW_TRIGGER_ID: &str = "delete-live-view-trigger";

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
                        PageNotice {
                            class: "h-full px-5",
                            role: "alert",
                            title: "Viewer state is unavailable",
                            message,
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
                        PageNotice {
                            class: "h-full px-5",
                            title: "No diff is open",
                            message: "Run a git-tools diff command to open a snapshot or live view.",
                            icon: rsx! {
                                FileDiff { size: 22 }
                            },
                        }
                    },
                    ViewerActiveState::Pending { .. } => rsx! {},
                    ViewerActiveState::Broken { code, message, .. } => rsx! {
                        PageNotice {
                            class: "h-full px-5",
                            role: "alert",
                            title: format!("Render stopped ({})", code.as_str()),
                            message: message.clone(),
                        }
                    },
                    ViewerActiveState::Error { message, .. } => rsx! {
                        PageNotice {
                            class: "h-full px-5",
                            role: "alert",
                            title: "Render failed",
                            message: message.clone(),
                        }
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
fn ReadyWorkspace(
    view: ReadSignal<ViewerActiveView>,
    shell: ReadSignal<ViewerShellLoad>,
) -> Element {
    let viewer = use_context::<ViewerContext>();
    let toast = use_toast();
    let mut delete_open = use_signal(|| false);
    let mut delete_pending = use_signal(|| false);
    let mut mobile_panel = use_signal(|| None::<MobilePanel>);
    let file_filter = use_signal(String::new);
    let mut path_filter_open = use_signal(|| false);
    let files_folded = use_signal(|| None::<bool>);
    let flashing_file = use_signal(|| None::<String>);
    let mut find_open = use_signal(|| false);
    let commit_pages = use_viewer_commit_pages(view);
    let keybindings = shell.with(|shell| match shell {
        ViewerShellLoad::Ready(shell) => shell.preferences.keybindings,
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => ViewerKeybindings::default(),
    });
    let workspace = super::use_diff_workspace_context(
        view,
        commit_pages.commits(),
        super::DiffWorkspaceSignals {
            file_filter,
            path_filter_open,
            files_folded,
            flashing_file,
            find_open,
        },
        true,
    );
    browser::use_window_keydown(move |event| {
        if native_keyboard_event_matches(&event, keybindings, ViewerKeybindingAction::SearchFiles) {
            event.prevent_default();
            super::path_filter::open_path_filter(workspace);
        } else if native_keyboard_event_matches(
            &event,
            keybindings,
            ViewerKeybindingAction::SearchTextInAllFiles,
        ) {
            event.prevent_default();
            path_filter_open.set(false);
            find_open.set(true);
            browser::focus_element("viewer-diff-find-input".to_owned());
        }
    });
    use_effect(move || {
        if path_filter_open() {
            mobile_panel.set(None);
            find_open.set(false);
        }
    });
    let (tab_id, identity) = view.with(|view| (view.identity.tab_id, view.identity));
    let ready_shell = shell.with(|shell| {
        let ViewerShellLoad::Ready(shell) = shell else {
            return None;
        };
        Some(
            shell
                .tabs
                .iter()
                .any(|tab| tab.id == tab_id && tab.kind == ViewerTabKind::Live),
        )
    });
    let commits_loading = commit_pages.is_loading();
    let commits_error = commit_pages.error().map(|error| error.message().to_owned());
    let commits_has_more = commit_pages.has_more();
    let onload_commits = use_callback(move |()| commit_pages.load_next());

    let onrefresh = move |_| viewer.refresh_tab(tab_id);

    let mut commit_selection = use_action(move |action: CommitSelectionAction| async move {
        let result = match action {
            CommitSelectionAction::FetchCommit(request) => {
                viewer_server::select_commit(request).await
            }
            CommitSelectionAction::UnselectCommit => {
                viewer_server::clear_commit_selection(ViewerTabRequest { tab_id }).await
            }
            CommitSelectionAction::NoAction => return Ok::<(), std::convert::Infallible>(()),
        };
        match result {
            Ok(shell) => viewer.replace_shell(shell),
            Err(error) => toast.error(error.message()),
        }
        Ok::<(), std::convert::Infallible>(())
    });
    let onclear_commit = use_callback(move |()| {
        commit_selection.call(CommitSelectionAction::UnselectCommit);
    });

    let onselect_commit = use_callback(move |id: CommitId| {
        let current_selection = view.peek().commit_selection.clone();
        let action = make_commit_selection_action(&current_selection, tab_id, id);
        match action {
            CommitSelectionAction::NoAction => {}
            action => {
                commit_selection.call(action);
            }
        }
    });

    let onnavigate = super::use_file_navigation(flashing_file);
    let onopen = move |file: gtl_wire::viewer::ViewerDiffFileId| {
        spawn(async move {
            if let Err(error) =
                viewer_server::open_diff_file(OpenViewerDiffFile { identity, file }).await
            {
                toast.error(error.message());
            }
        });
    };
    let Some(is_live) = ready_shell else {
        return rsx! {};
    };
    let (file_count, commit_count) = workspace
        .files
        .with(|files| (files.file_count(), files.commit_count()));
    let mobile_navigation = rsx! {
        WorkspaceMobileNavigation {
            files_trigger_id: "mobile-files-trigger",
            files_panel_id: "mobile-files-panel",
            commits_trigger_id: "mobile-commits-trigger",
            commits_panel_id: "mobile-commits-panel",
            file_count,
            commit_count,
            files_open: mobile_panel() == Some(MobilePanel::Files),
            commits_open: mobile_panel() == Some(MobilePanel::Commits),
            onfiles: move |_| mobile_panel.set(Some(MobilePanel::Files)),
            oncommits: move |_| mobile_panel.set(Some(MobilePanel::Commits)),
        }
    };
    let live_actions = is_live.then(|| {
        rsx! {
            LiveViewTitlebarActions {
                pending: viewer.render_command_pending(),
                onrefresh,
                ondelete: move |_| {
                    browser::hide_popover(LIVE_VIEW_ACTIONS_POPOVER_ID);
                    delete_open.set(true);
                },
            }
        }
    });
    rsx! {
        section { class: "h-full min-h-0 overflow-hidden",
            DiffWorkspaceDocument {
                diff_document: rsx! {
                    ClientDiffDocument { onopen }
                },
                onnavigate,
                mobile_navigation,
                live_actions,
                onselect_commit,
                onclear_commit,
                commits_loading,
                commits_error: commits_error.clone(),
                commits_has_more,
                onload_commits,
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
                details_popover_id_prefix: "mobile-commits-panel",
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
            trigger_id: DELETE_LIVE_VIEW_TRIGGER_ID,
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

#[component]
fn LiveViewTitlebarActions(
    pending: bool,
    onrefresh: EventHandler<MouseEvent>,
    ondelete: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "flex flex-none items-center gap-1",
            Button {
                class: "mobile:size-11 mobile:p-0",
                size: ButtonSize::Small,
                variant: ButtonVariant::Ghost,
                state: if pending { ButtonState::Loading } else { ButtonState::Enabled },
                aria_label: "Refresh diff",
                title: "Refresh diff",
                "data-testid": test_ids::LIVE_VIEW_REFRESH.value(),
                onclick: onrefresh,
                if !pending {
                    span {
                        class: "inline-flex flex-none mobile:[&_svg]:size-5",
                        aria_hidden: "true",
                        RefreshCw { size: 14 }
                    }
                }
                span { class: "mobile:hidden", "Refresh" }
            }
            IconPopover {
                id: LIVE_VIEW_ACTIONS_POPOVER_ID,
                aria_label: "Live view actions",
                placement: IconPopoverPlacement::TriggerEnd,
                icon: rsx! {
                    Ellipsis { size: 18 }
                },
                div { class: "grid gap-0.5 p-1.5",
                    button {
                        id: DELETE_LIVE_VIEW_TRIGGER_ID,
                        class: MENU_ACTION_HOST_CLASSES,
                        r#type: "button",
                        onclick: ondelete,
                        MenuActionContent {
                            icon: rsx! {
                                Trash2 { size: 16 }
                            },
                            label: "Delete live view",
                            description: "Remove this saved live view",
                        }
                    }
                }
            }
        }
    }
}
