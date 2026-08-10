use dioxus::prelude::*;
use gtl_contracts::viewer::{
    SetViewerPreference, ViewerActiveState, ViewerActiveView, ViewerCommitSelection,
    ViewerDiffChunkContinuation, ViewerDiffDensity, ViewerDiffLayout, ViewerDiffMaterialization,
    ViewerFileStatus, ViewerFileSummary, ViewerFooter, ViewerPreferences, ViewerShell,
    ViewerTabKind, ViewerTheme, ViewerViewIdentity,
};
use lucide_dioxus::{
    ChevronRight, CircleDot, FileDiff, GitCommitHorizontal, ListFilter, LoaderCircle, PanelLeft,
    RefreshCw,
};

use crate::{
    app::application_layout::{ViewerContext, ViewerShellLoad},
    entities::diffs::{
        DiffIslandAppendResult, DiffIslandBridge, DiffViewerApi, theme_value, view_identity_value,
    },
    shared::{
        bridge::ClientApiError,
        browser,
        ui::{
            AlertDialog, Badge, BadgeVariant, Button, ButtonSize, ButtonState, ButtonVariant,
            FloatingNotice, FloatingNoticeState, Popover, ScrollArea, Skeleton, TextInput,
            TextInputLabelVisibility,
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
    let mut files_folded = use_signal(|| false);
    let mut copy_context_enabled = use_signal(|| true);
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
                    files_folded: files_folded(),
                    copy_context_enabled: copy_context_enabled(),
                    onfold: move |folded| files_folded.set(folded),
                    oncontext: move |enabled| copy_context_enabled.set(enabled),
                }
                aside {
                    class: "col-start-1 row-start-2 hidden min-h-0 overflow-hidden border-r border-line bg-surface workspace:block",
                    aria_label: "Changed files",
                    FilesPanel {
                        view: view.clone(),
                        filter: file_filter(),
                        onfilter: move |value| file_filter.set(value),
                    }
                }
                DiffIsland {
                    identity: view.identity,
                    theme: preferences.theme,
                    title: view.title.clone(),
                    folded: files_folded(),
                    copy_context_enabled: copy_context_enabled(),
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
                        onclick: move |_| files_folded.set(!files_folded()),
                        if files_folded() {
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
#[component]
fn ViewTitlebar(
    view: ViewerActiveView,
    files_folded: bool,
    copy_context_enabled: bool,
    onfold: EventHandler<bool>,
    oncontext: EventHandler<bool>,
) -> Element {
    rsx! {
        header { class: "col-span-3 row-start-1 flex min-w-0 items-center gap-4 border-b border-line bg-surface px-5 py-3 tablet:flex-wrap tablet:gap-2.5 tablet:px-3 tablet:py-2.5 mobile:gap-1.5 mobile:px-2 mobile:py-2",
            div { class: "flex min-w-0 items-baseline gap-2 text-lg font-semibold tracking-tight mobile:text-base",
                span { class: "truncate",
                    "~/"
                    b { class: "font-bold text-acc", "{view.repository_name}" }
                }
                span { class: "flex-none self-center rounded-sm border border-acc-line bg-acc-soft px-2 py-0.5 text-xs font-medium text-acc",
                    "{view.title}"
                }
            }
            div { class: "flex min-w-0 items-center gap-1.5 text-ink-2 tablet:order-3 tablet:w-full",
                span { class: "truncate text-acc", "{view.branch}" }
                if !view.upstream.is_empty() {
                    span { class: "text-ink-3", "→" }
                    span { class: "truncate text-ink-3", "{view.upstream}" }
                }
            }
            if let Some(exclusions) = &view.exclusions {
                span {
                    class: "flex-none cursor-help whitespace-nowrap rounded-sm border border-del-line bg-del-bg px-2 py-0.5 text-xs font-semibold text-del",
                    title: exclusion_tooltip(exclusions),
                    {exclusion_label(exclusions)}
                }
            }
            div { class: "flex-1" }
            div { class: "flex items-center gap-2 mobile:hidden",
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    title: "Collapse or expand all files",
                    onclick: move |_| onfold.call(!files_folded),
                    if files_folded {
                        "Expand all"
                    } else {
                        "Collapse all"
                    }
                }
                Button {
                    size: ButtonSize::Small,
                    variant: if copy_context_enabled { ButtonVariant::Pressed } else { ButtonVariant::Outline },
                    aria_pressed: copy_context_enabled.to_string(),
                    title: "Prepend a commented path and line range when copying code",
                    onclick: move |_| oncontext.call(!copy_context_enabled),
                    "+ context"
                }
            }
        }
    }
}

fn exclusion_label(exclusions: &gtl_contracts::viewer::ViewerAppliedExclusions) -> String {
    let hidden_count = exclusions.hidden_paths.len();
    let extension_label = if exclusions.extensions.is_empty() {
        "configured".to_owned()
    } else {
        exclusions.extensions.join(", ")
    };
    format!(
        "{hidden_count} file{} hidden · {extension_label}",
        plural_suffix(hidden_count)
    )
}

fn exclusion_tooltip(exclusions: &gtl_contracts::viewer::ViewerAppliedExclusions) -> String {
    let mut tooltip = String::from("Hidden by git-tools config [diff.exclude]:");
    for path in &exclusions.hidden_paths {
        tooltip.push('\n');
        tooltip.push_str(path);
    }
    tooltip
}

const fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
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
        div { class: "flex min-w-0 flex-wrap items-center gap-3",
            div {
                class: "flex items-center gap-1",
                role: "group",
                aria_label: "Layout",
                span { class: "mr-1 font-bold tracking-wider text-ink-3 uppercase", "Layout" }
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.layout == ViewerDiffLayout::Unified { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.layout == ViewerDiffLayout::Unified).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Layout(ViewerDiffLayout::Unified)),
                    "Unified"
                }
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.layout == ViewerDiffLayout::Split { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.layout == ViewerDiffLayout::Split).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Layout(ViewerDiffLayout::Split)),
                    "Side by side"
                }
            }
            div {
                class: "flex items-center gap-1",
                role: "group",
                aria_label: "View",
                span { class: "mr-1 font-bold tracking-wider text-ink-3 uppercase", "View" }
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.density == ViewerDiffDensity::Compact { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.density == ViewerDiffDensity::Compact).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Density(ViewerDiffDensity::Compact)),
                    "Changes"
                }
                Button {
                    size: ButtonSize::Small,
                    variant: if preferences.render_options.density == ViewerDiffDensity::Full { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    aria_pressed: (preferences.render_options.density == ViewerDiffDensity::Full).to_string(),
                    onclick: move |_| onpreference.call(SetViewerPreference::Density(ViewerDiffDensity::Full)),
                    "Full file"
                }
            }
            if is_live {
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Ghost,
                    state: if pending { ButtonState::Loading } else { ButtonState::Enabled },
                    onclick: onrefresh,
                    "Refresh"
                }
                Button {
                    id: delete_trigger_id,
                    class: "ml-2",
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Destructive,
                    onclick: ondelete,
                    "Delete live view"
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
        Button {
            id,
            size: ButtonSize::Small,
            variant: ButtonVariant::Outline,
            onclick,
            span { aria_hidden: "true",
                match icon {
                    MobilePanel::Display => rsx! {
                        ListFilter { size: 14 }
                    },
                    MobilePanel::Files => rsx! {
                        PanelLeft { size: 14 }
                    },
                    MobilePanel::Commits => rsx! {
                        GitCommitHorizontal { size: 14 }
                    },
                }
            }
            "{label}"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkspaceLineTotals {
    added: u64,
    removed: u64,
}

impl WorkspaceLineTotals {
    fn from_files(files: &[ViewerFileSummary]) -> Self {
        Self {
            added: files.iter().map(|file| u64::from(file.added)).sum(),
            removed: files.iter().map(|file| u64::from(file.removed)).sum(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct WorkspaceFileTree {
    directories: Vec<(String, Self)>,
    files: Vec<(String, ViewerFileSummary)>,
}

impl WorkspaceFileTree {
    fn from_files(files: &[ViewerFileSummary]) -> Self {
        let mut root = Self::default();
        for file in files {
            root.insert(&file.path, file.clone());
        }
        root
    }

    fn insert(&mut self, path: &str, file: ViewerFileSummary) {
        let Some((directory_name, remainder)) = path.split_once('/') else {
            self.files.push((path.to_owned(), file));
            return;
        };
        let index = self
            .directories
            .iter()
            .position(|(name, _)| name == directory_name)
            .unwrap_or_else(|| {
                self.directories
                    .push((directory_name.to_owned(), Self::default()));
                self.directories.len() - 1
            });
        self.directories[index].1.insert(remainder, file);
    }
}

#[component]
fn FilesPanel(view: ViewerActiveView, filter: String, onfilter: EventHandler<String>) -> Element {
    let filter_normalized = filter.to_lowercase();
    let files = view
        .files
        .iter()
        .filter(|file| file.path.to_lowercase().contains(&filter_normalized))
        .cloned()
        .collect::<Vec<_>>();
    let totals = WorkspaceLineTotals::from_files(&view.files);
    let tree = WorkspaceFileTree::from_files(&files);

    rsx! {
        ScrollArea { class: "h-full min-h-0 overflow-auto bg-surface p-3 compact:p-2.5",
            div { class: "relative mb-3",
                TextInput {
                    label: "Filter files",
                    label_visibility: TextInputLabelVisibility::Hidden,
                    class: "h-9 py-2",
                    value: filter,
                    placeholder: "Filter files…  /",
                    oninput: move |event: FormEvent| onfilter.call(event.value()),
                }
            }
            div { class: "mx-1 mt-1.5 mb-2 flex justify-between tracking-wider text-ink-3 uppercase",
                span { "{view.commits_label} · {view.files.len()} file{plural_suffix(view.files.len())}" }
            }
            div { class: "mx-0.5 mb-3 flex flex-wrap gap-2",
                Badge { class: "px-2 py-0.5",
                    b { class: "font-bold text-ink", "{view.commits.len()}" }
                    span { class: "ml-1", "commit{plural_suffix(view.commits.len())}" }
                }
                Badge { class: "px-2 py-0.5", variant: BadgeVariant::Addition, "+{totals.added}" }
                Badge { class: "px-2 py-0.5", variant: BadgeVariant::Deletion, "−{totals.removed}" }
            }
            if files.is_empty() {
                p { class: "rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic",
                    "no files match this filter"
                }
            } else {
                WorkspaceFileTreeView { tree }
            }
        }
    }
}

#[component]
fn WorkspaceFileTreeView(tree: WorkspaceFileTree, #[props(default)] nested: bool) -> Element {
    rsx! {
        ul { class: if nested { "m-0 list-none p-0 pl-2.5" } else { "m-0 list-none p-0" },
            for (directory_name, directory) in tree.directories {
                li { class: "min-w-0",
                    details { class: "group", open: true,
                        summary { class: "flex cursor-pointer list-none items-center gap-1.5 rounded-sm px-1.5 py-0.5 leading-snug text-ink-3 hover:bg-surface-2 hover:text-ink active:bg-acc-soft focus-visible:-outline-offset-2 focus-visible:outline-2 focus-visible:outline-acc [&::-webkit-details-marker]:hidden",
                            span {
                                class: "flex-none transition-transform group-open:rotate-90 motion-reduce:transition-none",
                                aria_hidden: "true",
                                ChevronRight { size: 12 }
                            }
                            span { class: "min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap",
                                "{directory_name}"
                            }
                        }
                        WorkspaceFileTreeView { tree: directory, nested: true }
                    }
                }
            }
            for (file_name, file) in tree.files {
                {
                    let anchor_id = file.anchor_id.clone();
                    rsx! {
                        li { class: "min-w-0",
                            button {
                                class: match file.status {
                                    ViewerFileStatus::Added => {
                                        "flex w-full cursor-pointer items-center gap-1.5 rounded-sm border-0 bg-add-bg/40 px-1.5 py-0.5 text-left leading-snug text-ink-2 hover:bg-add-bg/60 hover:text-ink active:bg-add-bg focus-visible:-outline-offset-2 focus-visible:outline-2 focus-visible:outline-acc"
                                    }
                                    ViewerFileStatus::Deleted => {
                                        "flex w-full cursor-pointer items-center gap-1.5 rounded-sm border-0 bg-del-bg/40 px-1.5 py-0.5 text-left leading-snug text-ink-2 hover:bg-del-bg/60 hover:text-ink active:bg-del-bg focus-visible:-outline-offset-2 focus-visible:outline-2 focus-visible:outline-acc"
                                    }
                                    ViewerFileStatus::Renamed | ViewerFileStatus::Modified => {
                                        "flex w-full cursor-pointer items-center gap-1.5 rounded-sm border-0 bg-transparent px-1.5 py-0.5 text-left leading-snug text-ink-2 hover:bg-surface-2 hover:text-ink active:bg-acc-soft focus-visible:-outline-offset-2 focus-visible:outline-2 focus-visible:outline-acc"
                                    }
                                },
                                r#type: "button",
                                title: file.path.clone(),
                                onclick: move |_| DiffIslandBridge::scroll_to_file(anchor_id.clone()),
                                Badge {
                                    class: "size-4 min-h-0! flex-none px-0 leading-none font-bold",
                                    variant: file_status_badge(file.status),
                                    title: file_status_title(file.status),
                                    "{file_status_label(file.status)}"
                                }
                                span { class: "min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap",
                                    "{file_name}"
                                }
                            }
                        }
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

const fn file_status_title(status: ViewerFileStatus) -> &'static str {
    match status {
        ViewerFileStatus::Added => "Added",
        ViewerFileStatus::Deleted => "Deleted",
        ViewerFileStatus::Renamed => "Renamed",
        ViewerFileStatus::Modified => "Modified",
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
        ScrollArea { class: "h-full min-h-0 overflow-auto bg-surface p-3 compact:p-2.5",
            div { class: "flex items-start justify-between gap-2",
                div {
                    h3 { class: "mx-0.5 mt-1.5 mb-1 font-semibold tracking-wider text-ink-3 uppercase",
                        "{view.commits_label}"
                    }
                    p { class: "mx-0.5 mt-0 mb-3 flex items-center gap-1.5 text-ink-3",
                        span { class: "flex-none text-acc", aria_hidden: "true",
                            CircleDot { size: 8, fill: "currentColor" }
                        }
                        "hash = copy · hover = notes"
                    }
                }
                if selected_sha.is_some() {
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        state: if selection_pending { ButtonState::Disabled } else { ButtonState::Enabled },
                        onclick: move |_| onclear.call(()),
                        "Range"
                    }
                }
            }
            if let ViewerCommitSelection::Error { message, .. } = &view.commit_selection {
                p {
                    class: "mb-2 rounded-sm border border-del-line bg-del-bg px-2 py-2 text-del",
                    role: "alert",
                    "{message}"
                }
            }
            if view.commits.is_empty() {
                p { class: "rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic",
                    "no commits in range"
                }
            }
            for commit in &view.commits {
                {
                    let sha = commit.sha.clone();
                    let selected = selected_sha == Some(commit.sha.as_str());
                    rsx! {
                        button {
                            class: if selected { "relative ml-1.5 block w-[calc(100%_-_0.375rem)] cursor-pointer rounded-r-sm border-0 border-l-2 border-acc bg-acc-soft py-1.5 pr-2 pl-6 text-left focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-acc disabled:cursor-not-allowed disabled:opacity-50" } else { "relative ml-1.5 block w-[calc(100%_-_0.375rem)] cursor-pointer rounded-r-sm border-0 border-l-2 border-line-2 bg-transparent py-1.5 pr-2 pl-6 text-left hover:border-l-acc-line hover:bg-surface-2 active:bg-acc-soft focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-acc disabled:cursor-not-allowed disabled:opacity-50" },
                            r#type: "button",
                            aria_pressed: selected.to_string(),
                            disabled: selection_pending,
                            onclick: move |_| onselect.call(sha.clone()),
                            span {
                                class: if selected { "pointer-events-none absolute top-2 -left-4 flex size-5 items-center justify-center bg-surface text-acc" } else { "pointer-events-none absolute top-2 -left-4 flex size-5 items-center justify-center bg-surface text-line-2" },
                                aria_hidden: "true",
                                CircleDot { size: 10 }
                            }
                            span { class: "mb-1 flex min-w-0 items-center gap-1.5",
                                code { class: if selected { "rounded-sm border border-acc bg-acc px-1.5 py-0.5 text-xs text-bg" } else { "rounded-sm border border-acc-line bg-acc-soft px-1.5 py-0.5 text-xs text-acc" },
                                    "{commit.abbreviated_sha}"
                                }
                                if commit.is_merge {
                                    Badge { variant: BadgeVariant::Neutral, "merge" }
                                }
                                if !commit.date.is_empty() {
                                    time {
                                        class: "ml-auto truncate text-ink-3 tabular-nums",
                                        datetime: commit.iso.clone(),
                                        title: commit.iso.clone(),
                                        "{commit.date}"
                                    }
                                }
                            }
                            span { class: "block wrap-anywhere leading-normal text-ink-2", "{commit.subject}" }
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
    copy_context_enabled: bool,
) -> Element {
    let mut state = use_signal(|| DiffIslandState::Loading);
    let mut generation = use_signal(|| 0_u64);
    let mut mounted = use_signal(|| false);
    let mut active_identity = use_signal(|| identity);
    let mut folded_value = use_signal(|| folded);
    let mut copy_context_value = use_signal(|| copy_context_enabled);
    let mut open_file_error = use_signal(|| None::<ClientApiError>);

    use_effect(use_reactive((&folded,), move |(folded,)| {
        folded_value.set(folded);
        if mounted() {
            DiffIslandBridge::set_files_folded(folded);
        }
    }));

    use_effect(use_reactive(
        (&copy_context_enabled,),
        move |(copy_context_enabled,)| {
            copy_context_value.set(copy_context_enabled);
            if mounted() {
                DiffIslandBridge::set_copy_context_enabled(copy_context_enabled);
            }
        },
    ));

    use_effect(use_reactive((&identity,), move |(identity,)| {
        let request_generation = {
            let mut current = generation.write();
            *current += 1;
            *current
        };
        state.set(DiffIslandState::Loading);
        open_file_error.set(None);
        spawn(async move {
            let document = match DiffViewerApi::prepare_diff_document(identity).await {
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
            DiffIslandBridge::set_copy_context_enabled(copy_context_value());

            let ViewerDiffMaterialization::Loading { load_id } = document.materialization else {
                state.set(DiffIslandState::Complete);
                return;
            };
            state.set(DiffIslandState::Streaming);

            for _ in 0..DIFF_CHUNK_COUNT_MAX {
                let chunk = match DiffViewerApi::load_diff_chunk(identity, load_id).await {
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
                    if let Err(error) = DiffViewerApi::open_diff_file(identity, event.path).await {
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
        section {
            class: "relative col-start-2 row-start-2 min-h-0 min-w-0 overflow-hidden bg-bg",
            aria_label: "Rendered diff",
            if current_state.is_busy() {
                div {
                    class: "absolute inset-x-0 top-0 z-10 border-b border-acc-line bg-acc-soft px-3 py-1.5 text-center text-acc",
                    role: "status",
                    if current_state == DiffIslandState::Loading {
                        "Preparing diff"
                    } else {
                        "Loading diff rows"
                    }
                }
            }
            if let DiffIslandState::Error(error) = current_state {
                div {
                    class: "absolute inset-x-4 top-4 z-10 rounded-sm border border-del-line bg-del-bg px-3 py-2 text-del",
                    role: "alert",
                    "{error.message()}"
                }
            }
            if let Some(error) = open_file_error() {
                div {
                    class: "absolute inset-x-4 bottom-4 z-10 rounded-sm border border-del-line bg-del-bg px-3 py-2 text-del",
                    role: "alert",
                    "{error.message()}"
                }
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
fn Keybar(footer: ViewerFooter) -> Element {
    const KEY_CLASSES: &str =
        "rounded-sm border border-line-2 border-b-2 bg-sunk px-1.5 py-px font-mono text-ink-2";

    rsx! {
        footer {
            class: "col-span-3 row-start-3 flex items-center gap-4 overflow-hidden border-t border-line bg-surface px-5 py-2 text-ink-3",
            aria_label: "Keyboard shortcuts",
            span { class: "overflow-hidden text-ellipsis whitespace-nowrap text-ink-2",
                "{footer.command} "
                span { class: "text-ink-3", "{footer.note}" }
            }
            div { class: "flex-1" }
            span { class: "flex-none",
                kbd { class: KEY_CLASSES, "j" }
                " "
                kbd { class: KEY_CLASSES, "k" }
                " file"
            }
            span { class: "flex-none",
                kbd { class: KEY_CLASSES, "/" }
                " filter"
            }
            span { class: "flex-none",
                kbd { class: KEY_CLASSES, "alt+shift+c" }
                " fold all"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_contracts::viewer::{ViewerDiffFileId, ViewerFileStatus, ViewerFileSummary};

    use super::{WorkspaceFileTree, WorkspaceLineTotals};

    fn file(path: &str, added: u32, removed: u32) -> ViewerFileSummary {
        ViewerFileSummary {
            id: ViewerDiffFileId::for_index(0),
            path: path.to_owned(),
            anchor_id: format!("f-{}", path.replace(['/', '.'], "-")),
            added,
            removed,
            status: ViewerFileStatus::Modified,
            can_open_in_editor: true,
        }
    }

    #[test]
    fn changed_files_summary_retains_total_line_changes() {
        let files = [file("src/added.rs", 3, 1), file("src/removed.rs", 1, 5)];

        assert_eq!(
            WorkspaceLineTotals::from_files(&files),
            WorkspaceLineTotals {
                added: 4,
                removed: 6,
            }
        );
    }

    #[test]
    fn changed_files_tree_preserves_path_hierarchy() {
        let files = [
            file("crates/web/src/app.rs", 3, 1),
            file("crates/web/src/view.rs", 1, 5),
        ];

        let tree = WorkspaceFileTree::from_files(&files);

        assert_eq!(tree.directories[0].0, "crates");
        assert_eq!(tree.directories[0].1.directories[0].0, "web");
        assert_eq!(
            tree.directories[0].1.directories[0].1.directories[0]
                .1
                .files[0]
                .0,
            "app.rs"
        );
    }
}
