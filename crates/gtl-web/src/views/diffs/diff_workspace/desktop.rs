use dioxus::prelude::*;
use gtl_models::{
    diffs::CommitId,
    failure::{Failure, ProjectFailure},
    viewer::{ViewerKeybindingAction, ViewerKeybindings, ViewerTabId},
};
use gtl_wire::viewer::{
    CommitSelectionAction, OpenViewerDiffFile, ViewerActiveState, ViewerActiveView,
    ViewerTabRequest, make_commit_selection_action,
};
use lucide_dioxus::FileDiff;

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
        failure_message::failure_message,
        failure_notice::client_error_message,
        i18n::{t, use_language},
        keyboard::native_keyboard_event_matches,
        ui::{
            Button, ButtonSize, ButtonState, ButtonVariant, PageNotice, PanelDialog, Skeleton,
            use_toast,
        },
    },
    views::{
        diffs::ClientDiffDocument,
        projects::{ComparisonBranchEditor, ComparisonEditorTrigger},
    },
};

#[component]
pub(crate) fn DiffWorkspaceView(tab_id: Option<ViewerTabId>) -> Element {
    let language = use_language();
    let viewer = use_context::<ViewerContext>();
    let shell = viewer.shell();
    let activating = matches!(&*shell.read(), ViewerShellLoad::Ready(state)
        if tab_id.is_some() && tab_id != active_tab_id(&state.active));

    rsx! {
        document::Title { {t!(language, "document-title-viewer")} }
        main {
            class: "diff-workspace-main h-full min-h-0",
            aria_busy: activating.to_string(),
            "inert": activating.then_some(""),
            h1 { id: "workspace-heading", class: "sr-only", tabindex: "-1",
                {t!(language, "workspace-heading")}
            }
            match &*shell.read() {
                ViewerShellLoad::Loading => rsx! {
                    WorkspaceLoading {}
                },
                ViewerShellLoad::Error(error) => {
                    let message = client_error_message(error, language);
                    rsx! {
                        PageNotice {
                            class: "h-full px-5",
                            role: "alert",
                            title: t!(language, "workspace-unavailable"),
                            message,
                            Button {
                                class: "mx-auto mt-4",
                                variant: ButtonVariant::Outline,
                                onclick: move |_| viewer.reconnect(),
                                {t!(language, "action-try-again")}
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
    let language = use_language();
    rsx! {
        div {
            class: "diff-workspace-loading h-full min-h-0",
            role: "status",
            aria_label: t!(language, "workspace-loading"),
            div { class: "diff-workspace-loading-columns min-h-0 gap-px xl:grid-cols-[15rem_minmax(0,1fr)_16rem]",
                Skeleton { class: "hidden h-full rounded-none xl:block" }
                Skeleton { class: "h-full rounded-none" }
                Skeleton { class: "hidden h-full rounded-none xl:block" }
            }
            span { class: "sr-only", {t!(language, "workspace-loading")} }
        }
    }
}

#[component]
fn WorkspaceShell(shell: ReadSignal<ViewerShellLoad>) -> Element {
    let language = use_language();
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
            aria_label: t!(language, "workspace-active-diff"),
            "data-viewer-state": active_view_dom_state(&shell_state.active),
            div { class: "diff-workspace-shell-content min-h-0",
                match &shell_state.active {
                    ViewerActiveState::Empty => rsx! {
                        PageNotice {
                            class: "h-full px-5",
                            title: t!(language, "workspace-empty"),
                            message: t!(language, "workspace-empty-message"),
                            icon: rsx! {
                                FileDiff { size: 22 }
                            },
                        }
                    },
                    ViewerActiveState::Pending { .. } => rsx! {
                        WorkspaceLoading {}
                    },
                    ViewerActiveState::Broken { tab_id, failure } => {
                        let tab_id = *tab_id;
                        rsx! {
                            PageNotice {
                                class: "h-full px-5",
                                role: "alert",
                                title: t!(language, "workspace-source-unavailable"),
                                message: failure_message(failure, language),
                                div { class: "mx-auto mt-4 flex flex-wrap items-center justify-center gap-2",
                                    ModifiedFilesButton { tab_id, visible: false }
                                    Button {
                                        variant: ButtonVariant::Outline,
                                        onclick: move |_| viewer.refresh_tab(tab_id),
                                        {t!(language, "action-try-again")}
                                    }
                                }
                            }
                        }
                    }
                    ViewerActiveState::Error { tab_id, failure } => rsx! {
                        WorkspaceError { tab_id: *tab_id, failure: failure.clone() }
                    },
                    ViewerActiveState::Ready { .. } => rsx! {
                        ReadyWorkspace { view, shell }
                    },
                }
            }
        }
    }
}

const fn active_view_dom_state(active: &ViewerActiveState) -> &'static str {
    match active {
        ViewerActiveState::Empty => "empty",
        ViewerActiveState::Pending { .. } => "loading",
        ViewerActiveState::Ready { .. } => "ready",
        ViewerActiveState::Broken { .. } | ViewerActiveState::Error { .. } => "error",
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

#[cfg(test)]
mod tests {
    use gtl_models::failure::ViewerFailure;
    use gtl_wire::viewer::ViewerActiveState;

    use super::active_view_dom_state;
    use crate::test_support::{TestResult, viewer_active_view, viewer_tab_id};

    #[test]
    fn active_render_states_map_to_dom_states() -> TestResult {
        let tab_id = viewer_tab_id(7)?;
        assert_eq!(
            active_view_dom_state(&ViewerActiveState::Pending { tab_id }),
            "loading"
        );
        assert_eq!(
            active_view_dom_state(&ViewerActiveState::Ready {
                view: Box::new(viewer_active_view(tab_id)?),
            }),
            "ready"
        );
        assert_eq!(
            active_view_dom_state(&ViewerActiveState::Error {
                tab_id,
                failure: ViewerFailure::RenderFailed.into(),
            }),
            "error"
        );
        Ok(())
    }
}

#[component]
fn ReadyWorkspace(
    view: ReadSignal<ViewerActiveView>,
    shell: ReadSignal<ViewerShellLoad>,
) -> Element {
    let language = use_language();
    let viewer = use_context::<ViewerContext>();
    let toast = use_toast();
    let mut mobile_panel = use_signal(|| None::<MobilePanel>);
    let sidebars = use_context::<super::sidebars::SidebarControls>();
    let mut file_filter = use_signal(String::new);
    let mut path_filter_open = use_signal(|| false);
    let presentation = use_context::<crate::views::diffs::presentation::DiffPresentation>();
    let mut files_folded = use_signal(move || presentation.all_folded(view.peek().identity.tab_id));
    use_effect(move || {
        if let Some(command) = (presentation.fold_command)()
            && command.tab_id == view.peek().identity.tab_id
        {
            files_folded.set(Some(command.folded));
        }
    });
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
            .map(|tab| tab.live)
    });
    let commits_loading = commit_pages.is_loading();
    let commits_error = commit_pages
        .error()
        .map(|error| client_error_message(&error, language));
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
            Err(error) => toast.client_error(&error),
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
                toast.client_error(&error);
            }
        });
    };
    let Some(is_live) = ready_shell else {
        return rsx! {};
    };
    let (file_count, commit_count) = workspace
        .files
        .with(|files| (files.file_count(), files.commit_count()));
    let push_disabled = view.read().modified_files || view.read().commit_count == 0;
    let commits_actions = rsx! {
        div { class: "flex flex-none items-center",
            crate::views::push::ViewPushButton {
                id: "viewer-push-trigger",
                identity,
                disabled: push_disabled,
            }
            ModifiedFilesButton { tab_id, visible: view.read().modified_files }
        }
    };
    let mobile_navigation = rsx! {
        WorkspaceMobileNavigation {
            files_trigger_id: "mobile-files-trigger",
            files_panel_id: "mobile-files-panel",
            commits_trigger_id: "mobile-commits-trigger",
            commits_panel_id: "mobile-commits-panel",
            file_count,
            commit_count,
            commits_actions: rsx! {
                ModifiedFilesButton { tab_id, visible: view.read().modified_files }
            },
            files_open: mobile_panel() == Some(MobilePanel::Files),
            commits_open: mobile_panel() == Some(MobilePanel::Commits),
            onfiles: move |_| mobile_panel.set(Some(MobilePanel::Files)),
            oncommits: move |_| mobile_panel.set(Some(MobilePanel::Commits)),
        }
    };
    rsx! {
        section { class: "h-full min-h-0 overflow-hidden",
            DiffWorkspaceDocument {
                sidebars: (sidebars.visibility)(),
                ontoggle_sidebar: sidebars.toggle,
                diff_document: rsx! {
                    div { class: "relative h-full min-h-0 min-w-0",
                        if file_count == 0 {
                            PageNotice {
                                class: "h-full min-h-48 px-5",
                                title: t!(language, "workspace-no-changes"),
                                message: if is_live { t!(language, "workspace-no-changes-live-message") } else { t!(language, "workspace-no-changes-message") },
                            }
                        } else {
                            ClientDiffDocument { onopen }
                        }
                        super::review_actions::ReviewActions { identity, disabled: push_disabled }
                    }
                },
                onnavigate,
                mobile_navigation,
                commits_actions: commits_actions.clone(),
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
            title: t!(language, "workspace-changed-files"),
            onclose: move |()| mobile_panel.set(None),
            FilesPanel {
                onnavigate,
                filter_control: rsx! {
                    super::extension_filters::desktop::ExtensionFilters { id: "mobile-diff-extension-filters" }
                },
            }
        }
        PanelDialog {
            // TODO: organize this more intuitively. not obvious that this is where the mobile view is.
            id: "mobile-commits-panel",
            trigger_id: "mobile-commits-trigger",
            open: mobile_panel() == Some(MobilePanel::Commits),
            title: t!(language, "workspace-commits"),
            onclose: move |()| mobile_panel.set(None),
            WorkspaceCommitsPanel {
                details_popover_id_prefix: "mobile-commits-panel",
                actions: rsx! {
                    crate::views::push::ViewPushButton { id: "mobile-viewer-push-trigger", identity, disabled: push_disabled }
                },
                onselect: onselect_commit,
                loading: commits_loading,
                load_error: commits_error,
                has_more: commits_has_more,
                onloadmore: onload_commits,
            }
        }
    }
}

/// Explains why the active tab has no view and offers the fixes its failure allows.
#[component]
fn WorkspaceError(tab_id: ViewerTabId, failure: Failure) -> Element {
    let language = use_language();
    let viewer = use_context::<ViewerContext>();
    let (title, setting) = match &failure {
        Failure::Project(
            reason @ (ProjectFailure::ComparisonBranchMissing { .. }
            | ProjectFailure::NoCommonAncestor { .. }
            | ProjectFailure::RepositoryUnborn { .. }),
        ) => (
            t!(language, "projects-review-comparison-unavailable"),
            reason
                .comparison_setting()
                .map(|(project, branch)| (project.clone(), branch.clone())),
        ),
        _ => (t!(language, "tab-state-failed"), None),
    };
    rsx! {
        PageNotice {
            class: "h-full px-5",
            role: "alert",
            title,
            message: failure_message(&failure, language),
            div { class: "mx-auto mt-4 flex flex-wrap items-center justify-center gap-2",
                ModifiedFilesButton { tab_id, visible: false }
                if let Some((project, branch)) = setting {
                    ComparisonBranchEditor {
                        project,
                        branch,
                        trigger: ComparisonEditorTrigger::Labeled,
                        onsaved: move |()| viewer.refresh_tab(tab_id),
                    }
                }
                Button {
                    variant: ButtonVariant::Outline,
                    onclick: move |_| viewer.refresh_tab(tab_id),
                    {t!(language, "action-try-again")}
                }
            }
        }
    }
}

#[component]
fn ModifiedFilesButton(tab_id: ViewerTabId, visible: bool) -> Element {
    let language = use_language();
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
            Err(error) => toast.client_error(&error),
        }
        Ok::<(), std::convert::Infallible>(())
    });
    rsx! {
        Button {
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Toggle,
            class: "mobile:size-11",
            state: if action.pending() { ButtonState::Loading } else if viewer.actions_enabled() { ButtonState::Enabled } else { ButtonState::Disabled },
            icon: rsx! {
                lucide_dioxus::FilePenLine { size: 16 }
            },
            aria_label: t!(language, "workspace-modified-files"),
            aria_pressed: visible.to_string(),
            title: t!(language, "workspace-modified-files-hint"),
            onclick: move |_| {
                if !action.pending() {
                    action.call(!visible);
                }
            },
        }
    }
}
