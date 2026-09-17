use dioxus::prelude::*;
use gtl_models::{
    diffs::CommitId,
    viewer::{ViewerKeybindingAction, ViewerKeybindings, ViewerTabId},
};
use gtl_wire::viewer::{
    CommitSelectionAction, OpenViewerDiffFile, ViewerActiveState, ViewerActiveView,
    ViewerTabRequest, make_commit_selection_action,
};
use lucide_dioxus::{FileDiff, TriangleAlert};

use super::{
    DiffWorkspaceDocument, MobilePanel, WorkspaceMobileNavigation,
    commits_panel::WorkspaceCommitsPanel, files_panel::FilesPanel,
};
use crate::{
    app::{
        application_layout::{ViewerContext, ViewerShellLoad},
        application_router::active_tab_id,
    },
    entities::diffs::{use_viewer_commit_pages, viewer_server},
    shared::{
        browser,
        ui::{
            Button, ButtonSize, ButtonState, ButtonVariant, HoverPopover, HoverPopoverPlacement,
            PageNotice, PanelDialog, Skeleton, use_hover_popover, use_toast,
        },
    },
    views::diffs::{ClientDiffDocument, search_keybindings::native_keyboard_event_matches},
};

#[component]
pub(crate) fn DiffWorkspaceView(tab_id: Option<ViewerTabId>) -> Element {
    let viewer = use_context::<ViewerContext>();
    let shell = viewer.shell();
    let activating = matches!(&*shell.read(), ViewerShellLoad::Ready(state)
        if tab_id.is_some() && tab_id != active_tab_id(&state.active));

    rsx! {
        document::Title { "Viewer - git-tools" }
        main {
            class: "diff-workspace-main h-full min-h-0",
            aria_busy: activating.to_string(),
            "inert": activating.then_some(""),
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
            class: "diff-workspace-loading h-full min-h-0",
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
            div { class: "diff-workspace-loading-columns min-h-0 gap-px xl:grid-cols-[15rem_minmax(0,1fr)_16rem]",
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
    let viewer = use_context::<ViewerContext>();
    let view = use_hook(move || shell.map(ready_active_view));
    let shell_state = shell.read();
    let ViewerShellLoad::Ready(shell_state) = &*shell_state else {
        return rsx! {};
    };
    rsx! {
        section {
            id: "viewer-active-view",
            class: "diff-workspace-shell h-full min-h-0",
            role: "tabpanel",
            aria_label: "Active diff",
            div { class: "diff-workspace-shell-content min-h-0",
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
                    ViewerActiveState::Pending { .. } => rsx! {
                        WorkspaceLoading {}
                    },
                    ViewerActiveState::Broken { tab_id, code, message } => {
                        let tab_id = *tab_id;
                        rsx! {
                            PageNotice {
                                class: "h-full px-5",
                                role: "alert",
                                title: format!("Render stopped ({})", code.as_str()),
                                message: message.clone(),
                                ModifiedFilesButton { tab_id, visible: false }
                                Button {
                                    class: "mx-auto mt-4",
                                    variant: ButtonVariant::Outline,
                                    onclick: move |_| viewer.refresh_tab(tab_id),
                                    "Try again"
                                }
                            }
                        }
                    }
                    ViewerActiveState::Error { tab_id, message, .. } => {
                        let tab_id = *tab_id;
                        rsx! {
                            PageNotice {
                                class: "h-full px-5",
                                role: "alert",
                                title: "Render failed",
                                message: message.clone(),
                                ModifiedFilesButton { tab_id, visible: false }
                                Button {
                                    class: "mx-auto mt-4",
                                    variant: ButtonVariant::Outline,
                                    onclick: move |_| viewer.refresh_tab(tab_id),
                                    "Try again"
                                }
                            }
                        }
                    }
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
    let mut mobile_panel = use_signal(|| None::<MobilePanel>);
    let sidebars = super::sidebars::use_sidebar_controls();
    let mut file_filter = use_signal(String::new);
    let mut path_filter_open = use_signal(|| false);
    let presentation = use_context::<crate::views::diffs::presentation::DiffPresentation>();
    let mut files_folded = use_signal(move || presentation.all_folded(view.peek().identity.tab_id));
    let mut flashing_file = use_signal(|| None::<String>);
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
        for (action, sidebar, panel) in [
            (
                ViewerKeybindingAction::ToggleFilesSidebar,
                super::sidebars::Sidebar::Files,
                MobilePanel::Files,
            ),
            (
                ViewerKeybindingAction::ToggleCommitsSidebar,
                super::sidebars::Sidebar::Commits,
                MobilePanel::Commits,
            ),
        ] {
            if native_keyboard_event_matches(&event, keybindings, action) {
                event.prevent_default();
                if !event.repeat() {
                    if browser::workspace_is_wide() {
                        sidebars.toggle.call(sidebar);
                    } else {
                        mobile_panel.set(if mobile_panel() == Some(panel) {
                            None
                        } else {
                            Some(panel)
                        });
                    }
                }
                return;
            }
        }
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
        shell
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.kind.is_live())
    });
    let commits_loading = commit_pages.is_loading();
    let commits_error = commit_pages.error().map(|error| error.message().to_owned());
    let commits_has_more = commit_pages.has_more();
    let onload_commits = use_callback(move |()| commit_pages.load_next());

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
    let active_tab = use_memo(move || view.read().identity.tab_id);
    let mut previous_tab = use_signal(move || *active_tab.peek());
    use_effect(move || {
        let tab = active_tab();
        if *previous_tab.peek() == tab {
            return;
        }
        previous_tab.set(tab);
        commit_selection.cancel();
        mobile_panel.set(None);
        file_filter.set(String::new());
        path_filter_open.set(false);
        find_open.set(false);
        flashing_file.set(None);
        files_folded.set(presentation.all_folded(tab));
    });
    let onselect_commit = use_callback(move |id: CommitId| {
        if commit_selection.pending() {
            return;
        }
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
    let live_actions = Some(rsx! {
        ModifiedFilesButton { tab_id, visible: view.read().modified_files }
        if is_live {
            LiveViewWarning { tab_id }
        }
    });
    rsx! {
        section { class: "h-full min-h-0 overflow-hidden",
            DiffWorkspaceDocument {
                sidebars: (sidebars.visibility)(),
                keybindings,
                ontoggle_sidebar: sidebars.toggle,
                diff_document: rsx! {
                    if file_count == 0 {
                        PageNotice {
                            class: "h-full min-h-48 px-5",
                            title: "No changes",
                            message: "No changes in this comparison. Updates appear automatically when HEAD changes.",
                        }
                    } else {
                        ClientDiffDocument { onopen }
                    }
                },
                onnavigate,
                mobile_navigation,
                live_actions,
                onselect_commit,
                commits_loading,
                commits_error: commits_error.clone(),
                commits_has_more,
                onload_commits,
            }
        }

        PanelDialog {
            id: "mobile-files-panel",
            trigger_id: "mobile-files-trigger",
            open: mobile_panel() == Some(MobilePanel::Files),
            title: "Changed files",
            onclose: move |()| mobile_panel.set(None),
            FilesPanel { onnavigate }
        }
        PanelDialog {
            // TODO: organize this more intuitively. not obvious that this is where the mobile view is.
            id: "mobile-commits-panel",
            trigger_id: "mobile-commits-trigger",
            open: mobile_panel() == Some(MobilePanel::Commits),
            title: "Commits",
            onclose: move |()| mobile_panel.set(None),
            WorkspaceCommitsPanel {
                details_popover_id_prefix: "mobile-commits-panel",
                onselect: onselect_commit,
                loading: commits_loading,
                load_error: commits_error,
                has_more: commits_has_more,
                onloadmore: onload_commits,
            }
        }
    }
}

#[component]
fn LiveViewWarning(tab_id: ViewerTabId) -> Element {
    let viewer = use_context::<ViewerContext>();
    let errors = viewer.live_errors();
    let errors = errors.read();
    let Some(errors) = errors
        .get(&tab_id)
        .filter(|errors| !errors.entries().is_empty())
    else {
        return rsx! {};
    };
    rsx! {
        LiveViewWarningPopover { tab_id, errors: errors.entries().to_vec() }
    }
}

#[component]
fn LiveViewWarningPopover(
    tab_id: ViewerTabId,
    errors: Vec<crate::entities::diffs::live_errors::LiveError>,
) -> Element {
    let id = format!("live-view-{tab_id}-errors");
    let anchor = format!("--{id}");
    let hover = use_hover_popover(id.clone());
    rsx! {
        div {
            class: "relative flex flex-none",
            style: "anchor-name: {anchor};",
            onmouseenter: move |_| hover.pointer_enter.call(()),
            onmouseleave: move |_| hover.pointer_leave.call(()),
            onfocusin: move |_| hover.focus_enter.call(()),
            onfocusout: move |_| hover.focus_leave.call(()),
            Button {
                class: "mobile:size-11",
                size: ButtonSize::IconSmall,
                variant: ButtonVariant::Ghost,
                aria_label: "Live diff update warnings",
                aria_describedby: id.clone(),
                "data-testid": gtl_web_contracts::test_ids::LIVE_VIEW_WARNING.value(),
                span { class: "text-warn", aria_hidden: "true",
                    TriangleAlert { size: 16 }
                }
            }
            HoverPopover {
                id,
                anchor_name: anchor.clone(),
                aria_label: "Live diff update warnings",
                placement: HoverPopoverPlacement::Below,
                p { class: "text-xs font-semibold text-ink", "Recent update errors" }
                ul { class: "mt-2 grid gap-2 text-xs",
                    for error in &errors {
                        li { key: "{error.message}", class: "break-words",
                            p { "{error.message}" }
                            if error.occurrences > 1 {
                                p { class: "mt-0.5 text-ink-3", "Occurred {error.occurrences} times" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ModifiedFilesButton(tab_id: ViewerTabId, visible: bool) -> Element {
    let viewer = use_context::<ViewerContext>();
    let toast = use_toast();
    let mut action = use_action(move |visible: bool| async move {
        let result = async {
            viewer_server::set_modified_files(gtl_wire::viewer::SetViewerModifiedFiles {
                tab_id,
                visible,
            })
            .await?;
            viewer_server::get_shell().await
        }
        .await;
        match result {
            Ok(shell) => viewer.replace_shell(shell),
            Err(error) => toast.error(error.message()),
        }
        Ok::<(), std::convert::Infallible>(())
    });
    rsx! {
        Button {
            size: ButtonSize::Small,
            variant: ButtonVariant::Outline,
            state: if viewer.actions_enabled() { ButtonState::Enabled } else { ButtonState::Disabled },
            aria_label: "Modified files",
            aria_pressed: visible.to_string(),
            title: "Inspect current staged, unstaged, and untracked changes against HEAD",
            onclick: move |_| {
                if !action.pending() {
                    action.call(!visible);
                }
            },
            lucide_dioxus::FilePenLine { size: 15 }
            span { class: "mobile:hidden", "Modified files" }
        }
    }
}
