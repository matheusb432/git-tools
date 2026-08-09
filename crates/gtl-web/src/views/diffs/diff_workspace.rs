use dioxus::prelude::*;
use gtl_contracts::viewer::{
    SetViewerPreference, ViewerActiveState, ViewerActiveView, ViewerCommitSelection,
    ViewerDiffChunkContinuation, ViewerDiffDensity, ViewerDiffLayout, ViewerDiffMaterialization,
    ViewerFileStatus, ViewerPreferences, ViewerShell, ViewerTab, ViewerTabKind, ViewerTabState,
    ViewerTheme, ViewerViewIdentity,
};
use lucide_dioxus::{
    ChevronsDownUp, CircleCheck, Columns2, FileDiff, FolderTree, GitCommitHorizontal, ListFilter,
    LoaderCircle, PanelLeft, RefreshCw, Rows3, Search, Trash2, TriangleAlert, X,
};

use crate::{
    app::application_layout::{ViewerContext, ViewerShellLoad},
    entities::diffs::{
        DiffIslandAppendResult, DiffIslandBridge, ViewerApi, theme_from_value, theme_label,
        theme_value, view_identity_value,
    },
    shared::{
        bridge::ClientApiError,
        browser,
        ui::{
            AlertDialog, Badge, BadgeVariant, Button, ButtonSize, ButtonState, ButtonVariant,
            Popover, Skeleton, TextInput,
        },
    },
};

const DIFF_CHUNK_COUNT_MAX: usize = 16_384;

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
        main { class: "grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-bg",
            h1 { id: "workspace-heading", class: "sr-only", tabindex: "-1", "Diff viewer" }
            match shell {
                ViewerShellLoad::Loading => rsx! { WorkspaceLoading {} },
                ViewerShellLoad::Error(error) => {
                    let message = error.message();
                    rsx! {
                        section { class: "grid h-full place-content-center px-5 text-center", role: "alert",
                            p { class: "text-sm font-semibold text-ink", "Viewer state is unavailable" }
                            p { class: "mt-1 max-w-md text-xs leading-5 text-ink-2", "{message}" }
                            Button { class: "mx-auto mt-4", variant: ButtonVariant::Outline, onclick: move |_| viewer.reconnect(), "Try again" }
                        }
                    }
                },
                ViewerShellLoad::Ready(shell) => rsx! { WorkspaceShell { shell } },
            }
        }
    }
}

#[component]
fn WorkspaceLoading() -> Element {
    rsx! {
        div { class: "grid h-full min-h-0 grid-rows-[2.75rem_3rem_minmax(0,1fr)]", role: "status", aria_label: "Loading viewer",
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
    let viewer = use_context::<ViewerContext>();
    let active_tab_id = active_tab_id(&shell.active);
    let mut action_error = use_signal(|| None::<ClientApiError>);

    rsx! {
        ViewerTabs {
            tabs: shell.tabs.clone(),
            active_tab_id,
            onactivate: move |tab_id| {
                action_error.set(None);
                spawn(async move {
                    match ViewerApi::activate_tab(tab_id).await {
                        Ok(shell) => viewer.replace_shell(shell),
                        Err(error) => action_error.set(Some(error)),
                    }
                });
            },
            onclose: move |request: CloseTabRequest| {
                action_error.set(None);
                spawn(async move {
                    match ViewerApi::close_tab(request.tab_id).await {
                        Ok(shell) => {
                            viewer.replace_shell(shell);
                            if let Some(focus_id) = request.focus_tab_id {
                                browser::focus_element(tab_element_id(focus_id));
                            } else {
                                browser::focus_element("workspace-heading".to_owned());
                            }
                        }
                        Err(error) => action_error.set(Some(error)),
                    }
                });
            },
        }
        section { id: "viewer-active-view", class: "flex h-full min-h-0 flex-col overflow-hidden", role: "tabpanel", aria_label: "Active diff",
            if let Some(error) = action_error() {
                div { class: "border-b border-del-line bg-del-bg px-4 py-2 text-xs text-del", role: "alert", "{error.message()}" }
            }
            div { class: "min-h-0 flex-1 overflow-hidden",
                match shell.active {
                    ViewerActiveState::Empty => rsx! { EmptyWorkspace {} },
                    ViewerActiveState::Pending { .. } => rsx! { PendingWorkspace {} },
                    ViewerActiveState::Broken { code, message, .. } => rsx! {
                        WorkspaceFailure { title: format!("Render stopped ({code})"), message }
                    },
                    ViewerActiveState::Error { message, .. } => rsx! {
                        WorkspaceFailure { title: "Render failed".to_owned(), message }
                    },
                    ViewerActiveState::Ready { view } => {
                        let is_live = shell.tabs.iter().any(|tab| {
                            tab.id == view.identity.tab_id && tab.kind == ViewerTabKind::Live
                        });
                        rsx! {
                            ReadyWorkspace {
                                view: *view,
                                preferences: shell.preferences,
                                is_live,
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn EmptyWorkspace() -> Element {
    rsx! {
        section { class: "grid h-full place-content-center px-5 text-center",
            span { class: "mx-auto text-acc", aria_hidden: "true", FileDiff { size: 22 } }
            h2 { class: "mt-3 text-sm font-semibold text-ink", "No diff is open" }
            p { class: "mt-1 max-w-md text-xs leading-5 text-ink-2", "Run a git-tools diff command to open a snapshot or live view." }
        }
    }
}

#[component]
fn PendingWorkspace() -> Element {
    rsx! {
        section { class: "grid h-full place-content-center px-5 text-center", role: "status",
            span { class: "mx-auto animate-spin text-acc motion-reduce:animate-none", aria_hidden: "true", LoaderCircle { size: 20 } }
            h2 { class: "mt-3 text-sm font-semibold text-ink", "Rendering diff" }
            p { class: "mt-1 text-xs text-ink-2", "The viewer will update when the render is ready." }
        }
    }
}

#[component]
fn WorkspaceFailure(title: String, message: String) -> Element {
    rsx! {
        section { class: "grid h-full place-content-center px-5 text-center", role: "alert",
            h2 { class: "text-sm font-semibold text-ink", "{title}" }
            p { class: "mt-1 max-w-md text-xs leading-5 text-ink-2", "{message}" }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CloseTabRequest {
    tab_id: u64,
    focus_tab_id: Option<u64>,
}

#[component]
fn ViewerTabs(
    tabs: Vec<ViewerTab>,
    active_tab_id: Option<u64>,
    onactivate: EventHandler<u64>,
    onclose: EventHandler<CloseTabRequest>,
) -> Element {
    rsx! {
        div { class: "flex min-h-11 min-w-0 items-stretch gap-px overflow-x-auto border-b border-line bg-surface px-2 [scrollbar-color:var(--color-line-2)_transparent] [scrollbar-width:thin]", role: "tablist", aria_label: "Open diffs",
            if tabs.is_empty() {
                p { class: "self-center px-2 text-xs text-ink-3", "No open diffs" }
            }
            for tab in &tabs {
                {
                    let tab_id = tab.id;
                    let active = active_tab_id == Some(tab_id);
                    let focus_tab_id = close_focus_target(&tabs, tab_id);
                    let key_tabs = tabs.clone();
                    rsx! {
                        div { key: "{tab.id}", class: if active { "flex shrink-0 items-center border-b-2 border-acc bg-acc-soft" } else { "flex shrink-0 items-center border-b-2 border-transparent hover:bg-surface-2" },
                            button {
                                id: tab_element_id(tab_id),
                                class: "flex h-full max-w-64 min-w-0 cursor-pointer items-center gap-2 bg-transparent px-3 text-left text-xs text-ink-2 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-acc",
                                r#type: "button",
                                role: "tab",
                                aria_selected: active.to_string(),
                                aria_controls: "viewer-active-view",
                                tabindex: if active { "0" } else { "-1" },
                                onclick: move |_| onactivate.call(tab_id),
                                onkeydown: move |event| {
                                    let movement = match event.key() {
                                        Key::ArrowRight => Some(TabMovement::Next),
                                        Key::ArrowLeft => Some(TabMovement::Previous),
                                        Key::Home => Some(TabMovement::First),
                                        Key::End => Some(TabMovement::Last),
                                        _ => None,
                                    };
                                    if let Some(movement) = movement {
                                        event.prevent_default();
                                        let ids = key_tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
                                        if let Some(target) = tab_focus_target(&ids, tab_id, movement) {
                                            onactivate.call(target);
                                            browser::focus_element(tab_element_id(target));
                                        }
                                    }
                                },
                                TabStateIndicator { state: tab.state.clone() }
                                span { class: "truncate", "{tab.label}" }
                                if tab.kind == ViewerTabKind::Live {
                                    span { class: "rounded-sm border border-add-line bg-add-bg px-1 py-0.5 font-mono text-[8px] font-semibold uppercase tracking-[0.08em] text-add", "Live" }
                                }
                            }
                            Button {
                                size: ButtonSize::IconSmall,
                                variant: ButtonVariant::Ghost,
                                class: "mr-1",
                                aria_label: "Close {tab.label}",
                                title: "Close tab",
                                onclick: move |_| onclose.call(CloseTabRequest { tab_id, focus_tab_id }),
                                span { aria_hidden: "true", X { size: 13 } }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn active_tab_id(active: &ViewerActiveState) -> Option<u64> {
    match active {
        ViewerActiveState::Empty => None,
        ViewerActiveState::Pending { tab_id }
        | ViewerActiveState::Broken { tab_id, .. }
        | ViewerActiveState::Error { tab_id, .. } => Some(*tab_id),
        ViewerActiveState::Ready { view } => Some(view.identity.tab_id),
    }
}

fn tab_element_id(tab_id: u64) -> String {
    format!("viewer-tab-{tab_id}")
}

const fn tab_state_label(state: &ViewerTabState) -> &'static str {
    match state {
        ViewerTabState::Ready => "Ready",
        ViewerTabState::Pending => "Rendering",
        ViewerTabState::Broken => "Render stopped",
        ViewerTabState::Error => "Render failed",
    }
}

#[component]
fn TabStateIndicator(state: ViewerTabState) -> Element {
    let label = tab_state_label(&state);

    rsx! {
        span { class: "shrink-0", aria_hidden: "true",
            match state {
                ViewerTabState::Ready => rsx! {
                    span { class: "block text-add", CircleCheck { size: 13 } }
                },
                ViewerTabState::Pending => rsx! {
                    span { class: "block animate-spin text-acc motion-reduce:animate-none", LoaderCircle { size: 13 } }
                },
                ViewerTabState::Broken | ViewerTabState::Error => rsx! {
                    span { class: "block text-del", TriangleAlert { size: 13 } }
                },
            }
        }
        span { class: "sr-only", ", {label}" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabMovement {
    Next,
    Previous,
    First,
    Last,
}

fn tab_focus_target(ids: &[u64], current: u64, movement: TabMovement) -> Option<u64> {
    let current_index = ids.iter().position(|id| *id == current)?;
    let target_index = match movement {
        TabMovement::Next => (current_index + 1) % ids.len(),
        TabMovement::Previous => current_index.checked_sub(1).unwrap_or(ids.len() - 1),
        TabMovement::First => 0,
        TabMovement::Last => ids.len() - 1,
    };
    ids.get(target_index).copied()
}

fn close_focus_target(tabs: &[ViewerTab], closing: u64) -> Option<u64> {
    let index = tabs.iter().position(|tab| tab.id == closing)?;
    tabs.get(index + 1)
        .or_else(|| index.checked_sub(1).and_then(|previous| tabs.get(previous)))
        .map(|tab| tab.id)
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
    let mut files_folded = use_signal(|| false);
    let tab_id = view.identity.tab_id;

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
            match ViewerApi::select_commit(tab_id, sha).await {
                Ok(shell) => viewer.replace_shell(shell),
                Err(error) => action_error.set(Some(error)),
            }
        });
    };
    let onclear_commit = move |()| {
        action_error.set(None);
        spawn(async move {
            match ViewerApi::clear_commit_selection(tab_id).await {
                Ok(shell) => viewer.replace_shell(shell),
                Err(error) => action_error.set(Some(error)),
            }
        });
    };

    rsx! {
        section { class: "grid h-full min-h-0 grid-rows-[auto_auto_minmax(0,1fr)_auto] overflow-hidden",
            ViewTitlebar { view: view.clone() }
            div { class: "border-b border-line bg-surface px-3 py-2",
                div { class: "hidden items-center justify-between gap-3 xl:flex",
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
                div { class: "flex items-center gap-2 xl:hidden",
                    MobilePanelButton { id: "mobile-display-trigger", label: "Display", icon: MobilePanel::Display, onclick: move |_| mobile_panel.set(Some(MobilePanel::Display)) }
                    MobilePanelButton { id: "mobile-files-trigger", label: "Files", icon: MobilePanel::Files, onclick: move |_| mobile_panel.set(Some(MobilePanel::Files)) }
                    MobilePanelButton { id: "mobile-commits-trigger", label: "Commits", icon: MobilePanel::Commits, onclick: move |_| mobile_panel.set(Some(MobilePanel::Commits)) }
                    Button { class: "ml-auto", size: ButtonSize::IconSmall, variant: ButtonVariant::Ghost, aria_label: "Refresh diff", title: "Refresh diff", onclick: onrefresh,
                        if viewer.render_command_pending() {
                            span { class: "animate-spin motion-reduce:animate-none", aria_hidden: "true", LoaderCircle { size: 14 } }
                        } else {
                            span { aria_hidden: "true", RefreshCw { size: 14 } }
                        }
                    }
                }
                if let Some(error) = action_error().or_else(|| viewer.render_command_error()) {
                    p { class: "mt-2 text-xs text-del", role: "alert", "{error.message()}" }
                }
                if viewer.render_command_pending() {
                    p { class: "sr-only", role: "status", "Applying the latest viewer update" }
                }
            }

            div { class: "grid min-h-0 grid-cols-1 gap-px bg-line xl:grid-cols-[15rem_minmax(0,1fr)_17rem]",
                aside { class: "hidden min-h-0 overflow-hidden bg-surface xl:block", aria_label: "Changed files",
                    FilesPanel {
                        view: view.clone(),
                        filter: file_filter(),
                        folded: files_folded(),
                        onfilter: move |value| file_filter.set(value),
                        onfold: move |folded| files_folded.set(folded),
                    }
                }
                DiffIsland { identity: view.identity, theme: preferences.theme, title: view.title.clone(), folded: files_folded() }
                aside { class: "hidden min-h-0 overflow-hidden bg-surface xl:block", aria_label: "Commits",
                    CommitsPanel { view: view.clone(), onselect: onselect_commit, onclear: onclear_commit }
                }
            }
            Keybar {}
        }

        Popover {
            id: "mobile-display-panel",
            trigger_id: "mobile-display-trigger",
            open: mobile_panel() == Some(MobilePanel::Display),
            title: "Display controls",
            onclose: move |()| mobile_panel.set(None),
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
                folded: files_folded(),
                onfilter: move |value| file_filter.set(value),
                onfold: move |folded| files_folded.set(folded),
            }
        }
        Popover {
            id: "mobile-commits-panel",
            trigger_id: "mobile-commits-trigger",
            open: mobile_panel() == Some(MobilePanel::Commits),
            title: "Commits",
            onclose: move |()| mobile_panel.set(None),
            CommitsPanel { view: view.clone(), onselect: onselect_commit, onclear: onclear_commit }
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
                    match ViewerApi::delete_live_tab(tab_id).await {
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

#[component]
fn ViewTitlebar(view: ViewerActiveView) -> Element {
    rsx! {
        header { class: "flex min-w-0 items-start justify-between gap-4 border-b border-line bg-surface px-4 py-3",
            div { class: "min-w-0",
                div { class: "flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-1",
                    h2 { class: "truncate text-sm font-semibold text-ink", "{view.title}" }
                    p { class: "truncate font-mono text-[10px] text-ink-3", "{view.repository_name}" }
                }
                div { class: "mt-1 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[10px] text-ink-2",
                    span { "{view.branch}" }
                    if !view.upstream.is_empty() {
                        span { class: "text-ink-3", "-> {view.upstream}" }
                    }
                    code { class: "truncate text-acc", "{view.command.lead}{view.command.range}{view.command.trail}" }
                }
            }
            if let Some(exclusions) = &view.exclusions {
                span { class: "shrink-0 rounded-sm border border-line-2 bg-sunk px-2 py-1 font-mono text-[9px] text-ink-3", title: "Applied diff exclusions",
                    "-{exclusions.extensions.len()} ext / -{exclusions.hidden_paths.len()} paths"
                }
            }
        }
    }
}

#[component]
fn DisplayControls(
    preferences: ViewerPreferences,
    pending: bool,
    is_live: bool,
    delete_trigger_id: String,
    onpreference: EventHandler<SetViewerPreference>,
    onrefresh: EventHandler<MouseEvent>,
    ondelete: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "flex flex-wrap items-center gap-2",
            div { class: "flex items-center rounded-sm border border-line-2 bg-sunk p-0.5", aria_label: "Diff layout",
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.layout == ViewerDiffLayout::Unified { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.layout == ViewerDiffLayout::Unified).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Layout(ViewerDiffLayout::Unified)),
                    span { aria_hidden: "true", Rows3 { size: 13 } }
                    "Unified"
                }
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.layout == ViewerDiffLayout::Split { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.layout == ViewerDiffLayout::Split).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Layout(ViewerDiffLayout::Split)),
                    span { aria_hidden: "true", Columns2 { size: 13 } }
                    "Split"
                }
            }
            div { class: "flex items-center rounded-sm border border-line-2 bg-sunk p-0.5", aria_label: "Diff density",
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.density == ViewerDiffDensity::Compact { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.density == ViewerDiffDensity::Compact).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Density(ViewerDiffDensity::Compact)),
                    span { aria_hidden: "true", ListFilter { size: 13 } }
                    "Changes"
                }
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.density == ViewerDiffDensity::Full { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.density == ViewerDiffDensity::Full).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Density(ViewerDiffDensity::Full)),
                    span { aria_hidden: "true", FileDiff { size: 13 } }
                    "Full"
                }
            }
            label { class: "flex h-8 items-center gap-2 rounded-sm border border-line-2 bg-sunk px-2 text-xs text-ink-2",
                span { "Theme" }
                select {
                    class: "cursor-pointer bg-transparent font-mono text-[10px] text-ink outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc",
                    value: theme_value(preferences.theme),
                    onchange: move |event| {
                        if let Some(theme) = theme_from_value(&event.value()) {
                            onpreference.call(SetViewerPreference::Theme(theme));
                        }
                    },
                    for theme in [ViewerTheme::Dark, ViewerTheme::Light, ViewerTheme::Hearth, ViewerTheme::Mirage, ViewerTheme::Glacier, ViewerTheme::Noir, ViewerTheme::Graphite] {
                        option { value: theme_value(theme), "{theme_label(theme)}" }
                    }
                }
            }
        }
        div { class: "flex items-center gap-1",
            Button { size: ButtonSize::IconSmall, variant: ButtonVariant::Ghost, aria_label: "Refresh diff", title: "Refresh diff", onclick: onrefresh,
                if pending {
                    span { class: "animate-spin motion-reduce:animate-none", aria_hidden: "true", LoaderCircle { size: 14 } }
                } else {
                    span { aria_hidden: "true", RefreshCw { size: 14 } }
                }
            }
            if is_live {
                Button { id: delete_trigger_id, size: ButtonSize::IconSmall, variant: ButtonVariant::Ghost, aria_label: "Delete live view", title: "Delete live view", onclick: ondelete,
                    span { class: "text-del", aria_hidden: "true", Trash2 { size: 14 } }
                }
            }
        }
    }
}

#[component]
fn MobilePanelButton(
    id: String,
    label: String,
    icon: MobilePanel,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        Button { id, size: ButtonSize::Small, variant: ButtonVariant::Outline, onclick,
            span { aria_hidden: "true",
                match icon {
                    MobilePanel::Display => rsx! { ListFilter { size: 14 } },
                    MobilePanel::Files => rsx! { PanelLeft { size: 14 } },
                    MobilePanel::Commits => rsx! { GitCommitHorizontal { size: 14 } },
                }
            }
            "{label}"
        }
    }
}

#[component]
fn FilesPanel(
    view: ViewerActiveView,
    filter: String,
    folded: bool,
    onfilter: EventHandler<String>,
    onfold: EventHandler<bool>,
) -> Element {
    let filter_normalized = filter.to_lowercase();
    let files = view
        .files
        .iter()
        .filter(|file| file.path.to_lowercase().contains(&filter_normalized))
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        div { class: "grid h-full min-h-0 grid-rows-[auto_auto_minmax(0,1fr)]",
            header { class: "flex items-center justify-between gap-2 border-b border-line px-3 py-2.5",
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "text-acc", aria_hidden: "true", FolderTree { size: 14 } }
                    h3 { class: "truncate text-xs font-semibold text-ink", "Changed files" }
                    span { class: "font-mono text-[9px] tabular-nums text-ink-3", "{view.files.len()}" }
                }
                Button {
                    size: ButtonSize::IconSmall,
                    variant: ButtonVariant::Ghost,
                    aria_label: if folded { "Expand all files" } else { "Collapse all files" },
                    title: if folded { "Expand all files" } else { "Collapse all files" },
                    onclick: move |_| onfold.call(!folded),
                    span { aria_hidden: "true", ChevronsDownUp { size: 13 } }
                }
            }
            div { class: "border-b border-line px-3 py-2",
                div { class: "mb-1 flex items-center gap-1 font-mono text-[9px] uppercase tracking-[0.08em] text-ink-3",
                    span { aria_hidden: "true", Search { size: 11 } }
                    "Filter"
                }
                TextInput {
                    label: "Filter changed files",
                    class: "h-8",
                    value: filter,
                    placeholder: "src/render",
                    oninput: move |event: FormEvent| onfilter.call(event.value()),
                }
            }
            div { class: "min-h-0 overflow-auto p-1.5 [scrollbar-color:var(--color-line-2)_transparent] [scrollbar-width:thin]",
                if files.is_empty() {
                    p { class: "px-2 py-6 text-center text-xs text-ink-3", "No files match this filter." }
                }
                for file in files {
                    button {
                        class: "group grid w-full cursor-pointer grid-cols-[minmax(0,1fr)_auto] items-center gap-2 rounded-sm border border-transparent bg-transparent px-2 py-2 text-left hover:border-line hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-acc",
                        r#type: "button",
                        onclick: move |_| DiffIslandBridge::scroll_to_file(file.anchor_id.clone()),
                        span { class: "min-w-0",
                            span { class: "block truncate font-mono text-[11px] text-ink", "{file.path}" }
                            span { class: "mt-1 flex items-center gap-2 font-mono text-[9px] tabular-nums",
                                span { class: "text-add", "+{file.added}" }
                                span { class: "text-del", "-{file.removed}" }
                            }
                        }
                        Badge { variant: file_status_badge(file.status), "{file_status_label(file.status)}" }
                    }
                }
            }
        }
    }
}

const fn file_status_badge(status: ViewerFileStatus) -> BadgeVariant {
    match status {
        ViewerFileStatus::Added => BadgeVariant::Addition,
        ViewerFileStatus::Deleted => BadgeVariant::Deletion,
        ViewerFileStatus::Renamed => BadgeVariant::Accent,
        ViewerFileStatus::Modified => BadgeVariant::Neutral,
    }
}

const fn file_status_label(status: ViewerFileStatus) -> &'static str {
    match status {
        ViewerFileStatus::Added => "A",
        ViewerFileStatus::Deleted => "D",
        ViewerFileStatus::Renamed => "R",
        ViewerFileStatus::Modified => "M",
    }
}

#[component]
fn CommitsPanel(
    view: ViewerActiveView,
    onselect: EventHandler<String>,
    onclear: EventHandler<()>,
) -> Element {
    let selected_sha = match &view.commit_selection {
        ViewerCommitSelection::None => None,
        ViewerCommitSelection::Pending { sha }
        | ViewerCommitSelection::Ready { sha }
        | ViewerCommitSelection::Error { sha, .. } => Some(sha.as_str()),
    };
    let selection_pending = matches!(
        &view.commit_selection,
        ViewerCommitSelection::Pending { .. }
    );

    rsx! {
        div { class: "grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)]",
            header { class: "flex items-center justify-between gap-2 border-b border-line px-3 py-2.5",
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "text-acc", aria_hidden: "true", GitCommitHorizontal { size: 14 } }
                    h3 { class: "truncate text-xs font-semibold text-ink", "{view.commits_label}" }
                }
                if selected_sha.is_some() {
                    Button { size: ButtonSize::Small, variant: ButtonVariant::Ghost, state: if selection_pending { ButtonState::Disabled } else { ButtonState::Enabled }, onclick: move |_| onclear.call(()), "Range" }
                }
            }
            div { class: "min-h-0 overflow-auto p-1.5 [scrollbar-color:var(--color-line-2)_transparent] [scrollbar-width:thin]",
                if let ViewerCommitSelection::Error { message, .. } = &view.commit_selection {
                    p { class: "m-1 rounded-sm border border-del-line bg-del-bg px-2 py-2 text-xs text-del", role: "alert", "{message}" }
                }
                if view.commits.is_empty() {
                    p { class: "px-2 py-6 text-center text-xs text-ink-3", "No commits in this range." }
                }
                for commit in &view.commits {
                    {
                        let sha = commit.sha.clone();
                        let selected = selected_sha == Some(commit.sha.as_str());
                        rsx! {
                            button {
                                class: if selected { "grid w-full cursor-pointer gap-1 rounded-sm border border-acc-line bg-acc-soft px-2.5 py-2 text-left focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-acc" } else { "grid w-full cursor-pointer gap-1 rounded-sm border border-transparent bg-transparent px-2.5 py-2 text-left hover:border-line hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-acc" },
                                r#type: "button",
                                aria_pressed: selected.to_string(),
                                disabled: selection_pending,
                                onclick: move |_| onselect.call(sha.clone()),
                                span { class: "flex min-w-0 items-center justify-between gap-2",
                                    code { class: "font-mono text-[10px] font-semibold text-acc", "{commit.abbreviated_sha}" }
                                    time { class: "truncate font-mono text-[9px] text-ink-3", datetime: commit.iso.clone(), "{commit.date}" }
                                }
                                span { class: "line-clamp-2 text-[11px] leading-4 text-ink", "{commit.subject}" }
                                if commit.is_merge {
                                    span { class: "w-max rounded-sm border border-line-2 px-1 py-0.5 font-mono text-[8px] uppercase tracking-[0.08em] text-ink-3", "Merge" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffIslandState {
    Loading,
    Streaming,
    Complete,
    Error(ClientApiError),
}

impl DiffIslandState {
    const fn value(self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Streaming => "streaming",
            Self::Complete => "complete",
            Self::Error(_) => "error",
        }
    }

    const fn is_busy(self) -> bool {
        matches!(self, Self::Loading | Self::Streaming)
    }

    const fn chunks_complete(self) -> bool {
        matches!(self, Self::Complete)
    }

    const fn shows_document(self) -> bool {
        matches!(self, Self::Streaming | Self::Complete)
    }
}

#[component]
fn DiffIsland(
    identity: ViewerViewIdentity,
    theme: ViewerTheme,
    title: String,
    folded: bool,
) -> Element {
    let mut state = use_signal(|| DiffIslandState::Loading);
    let mut generation = use_signal(|| 0_u64);
    let mut mounted = use_signal(|| false);
    let mut active_identity = use_signal(|| identity);
    let mut folded_value = use_signal(|| folded);
    let mut open_file_error = use_signal(|| None::<ClientApiError>);

    use_effect(use_reactive((&folded,), move |(folded,)| {
        folded_value.set(folded);
        if mounted() {
            DiffIslandBridge::set_files_folded(folded);
        }
    }));

    use_effect(use_reactive((&identity,), move |(identity,)| {
        let request_generation = {
            let mut current = generation.write();
            *current += 1;
            *current
        };
        state.set(DiffIslandState::Loading);
        open_file_error.set(None);
        spawn(async move {
            let document = match ViewerApi::prepare_diff_document(identity).await {
                Ok(document) if document.identity == identity => document,
                Ok(_) => {
                    if generation() == request_generation {
                        state.set(DiffIslandState::Error(ClientApiError::Unavailable));
                    }
                    return;
                }
                Err(error) => {
                    if generation() == request_generation {
                        state.set(DiffIslandState::Error(error));
                    }
                    return;
                }
            };
            if generation() != request_generation {
                return;
            }

            let should_mount = !mounted();
            let chain = if should_mount {
                DiffIslandBridge::mount(&document).await
            } else {
                DiffIslandBridge::replace(&document).await
            };
            let chain = match chain {
                Ok(chain) => chain,
                Err(error) => {
                    if generation() == request_generation {
                        state.set(DiffIslandState::Error(error));
                    }
                    return;
                }
            };
            if generation() != request_generation {
                return;
            }
            if should_mount {
                mounted.set(true);
            }
            active_identity.set(identity);
            DiffIslandBridge::set_files_folded(folded_value());

            let ViewerDiffMaterialization::Loading { load_id } = document.materialization else {
                state.set(DiffIslandState::Complete);
                return;
            };
            state.set(DiffIslandState::Streaming);

            for _ in 0..DIFF_CHUNK_COUNT_MAX {
                let chunk = match ViewerApi::load_diff_chunk(identity, load_id).await {
                    Ok(chunk) if chunk.identity == identity => chunk,
                    Ok(_) => return,
                    Err(error) => {
                        if generation() == request_generation {
                            state.set(DiffIslandState::Error(error));
                        }
                        return;
                    }
                };
                if generation() != request_generation {
                    return;
                }
                let continuation = chunk.continuation;
                match DiffIslandBridge::append_chunk(&chain, &chunk).await {
                    Ok(DiffIslandAppendResult::Appended) => {}
                    Ok(DiffIslandAppendResult::Complete) => {
                        state.set(DiffIslandState::Complete);
                        return;
                    }
                    Ok(DiffIslandAppendResult::Stale) => return,
                    Ok(DiffIslandAppendResult::TargetMissing) | Err(_) => {
                        state.set(DiffIslandState::Error(ClientApiError::Unavailable));
                        return;
                    }
                }
                if continuation == ViewerDiffChunkContinuation::Complete {
                    state.set(DiffIslandState::Complete);
                    return;
                }
            }
            state.set(DiffIslandState::Error(ClientApiError::Unavailable));
        });
    }));

    let _open_files = use_future(move || async move {
        if let Err(error) = DiffIslandBridge::listen_for_open_files(
            || {},
            move |event| {
                let identity = active_identity();
                spawn(async move {
                    if let Err(error) = ViewerApi::open_diff_file(identity, event.path).await {
                        open_file_error.set(Some(error));
                    }
                });
            },
        )
        .await
        {
            open_file_error.set(Some(error));
        }
    });
    use_drop(DiffIslandBridge::destroy);

    let current_state = state();
    let identity_value = view_identity_value(identity);
    let shows_document = current_state.shows_document();

    rsx! {
        section { class: "relative min-h-0 min-w-0 overflow-hidden bg-bg", aria_label: "Rendered diff",
            if current_state.is_busy() {
                div { class: "absolute inset-x-0 top-0 z-10 border-b border-acc-line bg-acc-soft px-3 py-1.5 text-center text-[10px] text-acc", role: "status",
                    if current_state == DiffIslandState::Loading { "Preparing diff" } else { "Loading diff rows" }
                }
            }
            if let DiffIslandState::Error(error) = current_state {
                div { class: "absolute inset-x-4 top-4 z-10 rounded-sm border border-del-line bg-del-bg px-3 py-2 text-xs text-del", role: "alert", "{error.message()}" }
            }
            if let Some(error) = open_file_error() {
                div { class: "absolute inset-x-4 bottom-4 z-10 rounded-sm border border-del-line bg-del-bg px-3 py-2 text-xs text-del", role: "alert", "{error.message()}" }
            }
            div {
                id: "viewer-diff-island",
                class: if shows_document { "block h-full min-h-0 min-w-0 overflow-hidden" } else { "invisible pointer-events-none block h-full min-h-0 min-w-0 overflow-hidden" },
                role: "region",
                aria_label: "Rendered diff for {title}",
                aria_busy: current_state.is_busy().to_string(),
                aria_hidden: (!shows_document).then_some("true"),
                "data-view-state": current_state.value(),
                "data-view-identity": identity_value,
                "data-chunks-complete": current_state.chunks_complete().to_string(),
                "data-theme": theme_value(theme),
            }
        }
    }
}

#[component]
fn Keybar() -> Element {
    rsx! {
        footer { class: "flex min-h-8 items-center gap-4 overflow-x-auto border-t border-line bg-surface px-3 font-mono text-[9px] text-ink-3", aria_label: "Keyboard shortcuts",
            span { class: "shrink-0", kbd { class: "text-ink", "<- ->" } " tabs" }
            span { class: "shrink-0", kbd { class: "text-ink", "Home End" } " tab edges" }
            span { class: "ml-auto hidden shrink-0 text-ink-2 sm:block", "Server-rendered rows" }
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_contracts::viewer::ViewerTabState;

    use super::{TabMovement, close_focus_target, tab_focus_target, tab_state_label};

    #[test]
    fn tab_focus_wraps_and_supports_edges() {
        let ids = [4, 8, 15];

        assert_eq!(tab_focus_target(&ids, 15, TabMovement::Next), Some(4));
        assert_eq!(tab_focus_target(&ids, 4, TabMovement::Previous), Some(15));
        assert_eq!(tab_focus_target(&ids, 8, TabMovement::First), Some(4));
        assert_eq!(tab_focus_target(&ids, 8, TabMovement::Last), Some(15));
    }

    #[test]
    fn final_tab_close_targets_the_workspace_heading() {
        let tabs = [super::ViewerTab {
            id: 4,
            label: "Only diff".to_owned(),
            kind: super::ViewerTabKind::Snapshot,
            state: ViewerTabState::Ready,
        }];

        assert_eq!(close_focus_target(&tabs, 4), None);
    }

    #[test]
    fn every_tab_state_has_a_non_color_label() {
        assert_eq!(tab_state_label(&ViewerTabState::Ready), "Ready");
        assert_eq!(tab_state_label(&ViewerTabState::Pending), "Rendering");
        assert_eq!(tab_state_label(&ViewerTabState::Broken), "Render stopped");
        assert_eq!(tab_state_label(&ViewerTabState::Error), "Render failed");
    }
}
