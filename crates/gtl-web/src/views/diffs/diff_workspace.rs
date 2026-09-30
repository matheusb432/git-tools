use dioxus::prelude::*;
#[cfg(feature = "component-preview")]
use gtl_models::viewer::{ViewerKeybindingAction, ViewerKeybindings};
use gtl_models::{diffs::CommitId, settings::DiffFilesSort, viewer::ViewerTabId};
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::{ViewerActiveView, ViewerCommitSummary};
#[cfg(feature = "component-preview")]
use gtl_wire::viewer::{ViewerCommitSelection, ViewerDiffFileId};
use lucide_dioxus::{Files, GitCommitHorizontal};

use self::{
    commits_panel::WorkspaceCommitsPanel,
    file_search::{WorkspaceFileMatches, use_workspace_file_matches},
    files_panel::{FilesPanel, WorkspaceFilesModel},
};
#[cfg(feature = "component-preview")]
use super::client_diff_document::search_bar::{DiffSearchBar, DiffSearchScope};
use crate::shared::{
    i18n::{t, use_language},
    ui::{Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant, CountBadge},
};
#[cfg(feature = "component-preview")]
use crate::{
    entities::diffs::ClientDiffWorkspace,
    shared::{browser, keyboard::keyboard_event_matches, ui::PanelDialog},
    views::diffs::client_diff_document::PreviewDiffDocument,
};

pub(crate) mod commits_panel;
mod desktop;
pub(crate) mod file_filters;
mod file_search;
mod files_panel;
pub(crate) mod files_sort;
pub(super) mod panel_scroll;
mod path_filter;
pub(crate) mod sidebars;
mod titlebar;

pub(crate) use desktop::DiffWorkspaceView;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileFoldCommand {
    pub(crate) tab_id: ViewerTabId,
    pub(crate) folded: bool,
}

#[derive(Clone, Copy)]
pub(super) struct DiffWorkspaceContext {
    pub(super) view: ReadSignal<ViewerActiveView>,
    commits: ReadStore<Vec<ViewerCommitSummary>>,
    files: Memo<WorkspaceFilesModel>,
    files_sort: ReadSignal<DiffFilesSort>,
    file_filter: Signal<String>,
    path_filter_open: Signal<bool>,
    file_matches: Memo<WorkspaceFileMatches>,
    pub(super) files_folded: Signal<Option<bool>>,
    pub(super) fold_command: Signal<Option<FileFoldCommand>>,
    pub(super) flashing_file: Signal<Option<String>>,
    pub(super) find_open: Signal<bool>,
}

#[derive(Clone, Copy)]
struct DiffWorkspaceSignals {
    files_sort: ReadSignal<DiffFilesSort>,
    file_filter: Signal<String>,
    path_filter_open: Signal<bool>,
    files_folded: Signal<Option<bool>>,
    flashing_file: Signal<Option<String>>,
    find_open: Signal<bool>,
}

fn use_diff_workspace_context(
    view: ReadSignal<ViewerActiveView>,
    commits: ReadStore<Vec<ViewerCommitSummary>>,
    signals: DiffWorkspaceSignals,
    server_owned_file_search: bool,
) -> DiffWorkspaceContext {
    let DiffWorkspaceSignals {
        files_sort,
        file_filter,
        path_filter_open,
        files_folded,
        flashing_file,
        find_open,
    } = signals;
    let file_matches =
        use_workspace_file_matches(view, file_filter.into(), server_owned_file_search);
    let files = use_memo(move || WorkspaceFilesModel::new(&view.read(), files_sort()));
    let context = DiffWorkspaceContext {
        view,
        commits,
        files,
        files_sort,
        file_filter,
        path_filter_open,
        file_matches,
        files_folded,
        fold_command: use_hook(|| {
            try_consume_context::<super::presentation::DiffPresentation>().map_or_else(
                || Signal::new(None),
                |presentation| presentation.fold_command,
            )
        }),
        flashing_file,
        find_open,
    };
    use_context_provider(|| context);
    context
}

fn use_static_diff_workspace_context(
    view: ReadSignal<ViewerActiveView>,
    commits: ReadStore<Vec<ViewerCommitSummary>>,
    path_filter_open_initial: bool,
) -> DiffWorkspaceContext {
    let signals = DiffWorkspaceSignals {
        files_sort: use_signal(DiffFilesSort::default).into(),
        file_filter: use_signal(String::new),
        path_filter_open: use_signal(move || path_filter_open_initial),
        files_folded: use_signal(|| None::<bool>),
        flashing_file: use_signal(|| None::<String>),
        find_open: use_signal(|| false),
    };
    use_diff_workspace_context(view, commits, signals, false)
}

fn use_file_navigation(mut flashing_file: Signal<Option<String>>) -> Callback<String> {
    let mut clear_file_flash = use_action(move || async move {
        dioxus_sdk_time::sleep(std::time::Duration::from_millis(1_200)).await;
        flashing_file.set(None);
        Ok::<(), std::convert::Infallible>(())
    });
    use_callback(move |anchor_id: String| {
        crate::shared::browser::scroll_to_file(&anchor_id);
        flashing_file.set(Some(anchor_id));
        clear_file_flash.call();
    })
}

pub(super) fn use_workspace_context() -> DiffWorkspaceContext {
    use_context::<DiffWorkspaceContext>()
}

#[component]
fn WorkspaceMobileNavigation(
    files_trigger_id: String,
    files_panel_id: String,
    commits_trigger_id: String,
    commits_panel_id: String,
    file_count: usize,
    commit_count: usize,
    #[props(default)] files_open: bool,
    #[props(default)] commits_open: bool,

    #[props(default)] preview_visible: bool,
    onfiles: EventHandler<MouseEvent>,
    oncommits: EventHandler<MouseEvent>,
    commits_actions: Option<Element>,
) -> Element {
    let language = use_language();
    let navigation_classes = if preview_visible {
        "diff-workspace-mobile-navigation"
    } else {
        "diff-workspace-mobile-navigation workspace:hidden"
    };

    rsx! {
        nav {
            class: navigation_classes,
            aria_label: t!(language, "workspace-panels"),
            Button {
                id: files_trigger_id,
                class: "flex min-h-11 min-w-0 justify-center gap-2 rounded-none border-0 border-r px-3 focus-visible:-outline-offset-2",
                layout: ButtonLayout::Content,
                size: ButtonSize::Content,
                variant: ButtonVariant::Ghost,
                state: if file_count > 0 { ButtonState::Enabled } else { ButtonState::Disabled },
                aria_label: t!(language, "workspace-changed-files"),
                aria_controls: files_panel_id,
                aria_expanded: files_open.to_string(),
                aria_haspopup: "dialog",

                onclick: onfiles,
                span {
                    class: "inline-flex size-5 flex-none items-center justify-center [&_svg]:size-5",
                    aria_hidden: "true",
                    Files { size: 20 }
                }
                span { class: "font-semibold", {t!(language, "workspace-files")} }
                CountBadge { count: file_count }
            }
            Button {
                id: commits_trigger_id,
                class: "flex min-h-11 min-w-0 justify-center gap-2 rounded-none border-0 px-3 focus-visible:-outline-offset-2",
                layout: ButtonLayout::Content,
                size: ButtonSize::Content,
                variant: ButtonVariant::Ghost,
                state: if commit_count > 0 { ButtonState::Enabled } else { ButtonState::Disabled },
                aria_label: t!(language, "workspace-commits"),
                aria_controls: commits_panel_id,
                aria_expanded: commits_open.to_string(),
                aria_haspopup: "dialog",

                onclick: oncommits,
                span {
                    class: "inline-flex size-5 flex-none items-center justify-center [&_svg]:size-5",
                    aria_hidden: "true",
                    GitCommitHorizontal { size: 20 }
                }
                span { class: "font-semibold", {t!(language, "workspace-commits")} }
            }
            if let Some(actions) = commits_actions {
                div { class: "flex items-center pr-1", {actions} }
            }
        }
    }
}

#[cfg(feature = "component-preview")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewMobilePanel {
    Files,
    Commits,
}

#[cfg(feature = "component-preview")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PreviewDiffSearch {
    #[default]
    Closed,
    AllFiles,
    Paths,
}

#[cfg(feature = "component-preview")]
const PREVIEW_DIFF_SEARCH_INPUT_ID: &str = "preview-diff-search";

#[cfg(feature = "component-preview")]
#[component]
pub(crate) fn PreviewDiffWorkspace(
    mut view: ViewerActiveView,
    mut workspace: ClientDiffWorkspace,
    #[props(default)] mobile: bool,
    #[props(default)] initial_search: PreviewDiffSearch,
    keybindings: ViewerKeybindings,
) -> Element {
    let language = use_language();
    let markup = PreviewViewMarkup::new(view.identity.tab_id);
    for file in &mut view.files {
        file.anchor_id = markup.file_target_id(&file.id);
    }
    for file in &mut workspace.files {
        file.summary.anchor_id = markup.file_target_id(&file.summary.id);
    }
    let commits = std::mem::take(&mut view.commits);
    let mut view = use_signal(move || view);
    let commits = use_store(move || commits);
    let context = use_static_diff_workspace_context(
        view.into(),
        commits.into(),
        initial_search == PreviewDiffSearch::Paths,
    );
    let onnavigate = use_file_navigation(context.flashing_file);
    let mut mobile_panel = use_signal(|| None::<PreviewMobilePanel>);
    let mut search_scope = use_signal(move || match initial_search {
        PreviewDiffSearch::Closed | PreviewDiffSearch::Paths => None,
        PreviewDiffSearch::AllFiles => Some(DiffSearchScope::AllFiles),
    });
    let mut path_filter_open = context.path_filter_open;
    use_effect(move || {
        if path_filter_open() {
            mobile_panel.set(None);
            search_scope.set(None);
        }
    });
    let mut search_query = use_signal(|| "settings".to_owned());
    let (file_count, commit_count) = context
        .files
        .with(|files| (files.file_count(), files.commit_count()));
    let onselect_commit = use_callback(move |id: CommitId| {
        let mut current = view.write();
        current.commit_selection = match &current.commit_selection {
            ViewerCommitSelection::Ready { id: selected } if selected == &id => {
                ViewerCommitSelection::None
            }
            ViewerCommitSelection::None
            | ViewerCommitSelection::Pending { .. }
            | ViewerCommitSelection::Ready { .. }
            | ViewerCommitSelection::Error { .. } => ViewerCommitSelection::Ready { id },
        };
    });
    let open_all_files_search = use_callback(move |()| {
        path_filter_open.set(false);
        search_scope.set(Some(DiffSearchScope::AllFiles));
        browser::focus_element(PREVIEW_DIFF_SEARCH_INPUT_ID.to_owned());
    });
    let search_overlay = search_scope().map(|scope| {
        let status_message = preview_search_status(&search_query());
        rsx! {
            DiffSearchBar {
                input_id: PREVIEW_DIFF_SEARCH_INPUT_ID,
                scope,
                query: search_query(),
                status_message,
                navigation_enabled: !search_query().is_empty(),
                maxlength: None,
                onquerychange: move |value| search_query.set(value),
                onprevious: move |()| {},
                onnext: move |()| {},
                onclose: move |()| search_scope.set(None),
            }
        }
    });

    rsx! {
        if mobile {
            section {
                id: "workspace-heading",
                tabindex: "-1",
                class: "diff-workspace-mobile-grid h-full min-h-0",
                onkeydown: move |event: KeyboardEvent| {
                    if keyboard_event_matches(
                        &event,
                        keybindings,
                        ViewerKeybindingAction::SearchFiles,
                    ) {
                        event.prevent_default();
                        path_filter::open_path_filter(context);
                    } else if keyboard_event_matches(
                        &event,
                        keybindings,
                        ViewerKeybindingAction::SearchTextInAllFiles,
                    ) {
                        event.prevent_default();
                        open_all_files_search.call(());
                    }
                },
                path_filter::PathFilter { onnavigate }
                WorkspaceMobileNavigation {
                    files_trigger_id: "preview-mobile-files-trigger",
                    files_panel_id: "preview-mobile-files-panel",
                    commits_trigger_id: "preview-mobile-commits-trigger",
                    commits_panel_id: "preview-mobile-commits-panel",
                    file_count,
                    commit_count,
                    files_open: mobile_panel() == Some(PreviewMobilePanel::Files),
                    commits_open: mobile_panel() == Some(PreviewMobilePanel::Commits),
                    preview_visible: true,
                    onfiles: move |_| mobile_panel.set(Some(PreviewMobilePanel::Files)),
                    oncommits: move |_| mobile_panel.set(Some(PreviewMobilePanel::Commits)),
                }
                div { class: "diff-workspace-mobile-content min-h-0",
                    PreviewDiffDocument { workspace: workspace.clone(), overlay: search_overlay }
                }
            }
            PanelDialog {
                id: "preview-mobile-files-panel",
                trigger_id: "preview-mobile-files-trigger",
                open: mobile_panel() == Some(PreviewMobilePanel::Files),
                title: t!(language, "workspace-changed-files"),
                onclose: move |()| mobile_panel.set(None),
                FilesPanel { onnavigate }
            }
            PanelDialog {
                id: "preview-mobile-commits-panel",
                trigger_id: "preview-mobile-commits-trigger",
                open: mobile_panel() == Some(PreviewMobilePanel::Commits),
                title: t!(language, "workspace-commits"),
                onclose: move |()| mobile_panel.set(None),
                WorkspaceCommitsPanel {
                    details_popover_id_prefix: "preview-mobile-commits-panel",
                    onselect: onselect_commit,
                }
            }
        } else {
            section {
                id: "workspace-heading",
                tabindex: "-1",
                class: "diff-workspace-desktop-grid h-full min-h-0",
                onkeydown: move |event: KeyboardEvent| {
                    if keyboard_event_matches(
                        &event,
                        keybindings,
                        ViewerKeybindingAction::SearchFiles,
                    ) {
                        event.prevent_default();
                        path_filter::open_path_filter(context);
                    } else if keyboard_event_matches(
                        &event,
                        keybindings,
                        ViewerKeybindingAction::SearchTextInAllFiles,
                    ) {
                        event.prevent_default();
                        open_all_files_search.call(());
                    }
                },
                path_filter::PathFilter { onnavigate }
                aside {
                    class: "diff-workspace-panel min-h-0 diff-workspace-files-panel",
                    aria_label: t!(language, "workspace-changed-files"),
                    FilesPanel { onnavigate }
                }
                div { class: "col-start-2 row-start-1 min-h-0 overflow-hidden",
                    PreviewDiffDocument { workspace, overlay: search_overlay }
                }
                aside {
                    class: "diff-workspace-panel min-h-0 diff-workspace-commits-panel",
                    aria_label: t!(language, "workspace-commits"),
                    WorkspaceCommitsPanel {
                        details_popover_id_prefix: "preview-desktop-commits-panel",
                        onselect: onselect_commit,
                    }
                }
            }
        }
    }
}

#[cfg(feature = "component-preview")]
fn preview_search_status(query: &str) -> String {
    if query.is_empty() {
        return "Type to search code.".to_owned();
    }
    "9 matches in 3 files".to_owned()
}

#[cfg(feature = "component-preview")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreviewViewMarkup {
    tab_id: ViewerTabId,
}

#[cfg(feature = "component-preview")]
impl PreviewViewMarkup {
    fn new(tab_id: ViewerTabId) -> Self {
        Self { tab_id }
    }

    fn target_prefix(self) -> String {
        format!("preview-view-{}", self.tab_id)
    }

    fn file_target_id(self, file_id: &ViewerDiffFileId) -> String {
        format!("{}-{}", self.target_prefix(), file_id.as_str())
    }
}

#[component]
fn DiffWorkspaceDocument(
    #[props(default)] sidebars: gtl_models::viewer::ViewerSidebarVisibility,
    diff_document: Element,
    ontoggle_sidebar: Option<EventHandler<sidebars::Sidebar>>,
    onnavigate: EventHandler<String>,
    mobile_navigation: Option<Element>,
    commits_actions: Option<Element>,
    onselect_commit: Option<EventHandler<CommitId>>,
    #[props(default)] commits_loading: bool,
    commits_error: Option<String>,
    #[props(default)] commits_has_more: bool,
    onload_commits: Option<EventHandler<()>>,
) -> Element {
    let details_popover_id_prefix = "workspace-commits-panel".to_owned();
    let tab_id = use_workspace_context().view.read().identity.tab_id;
    rsx! {
        div {
            class: "diff-workspace-grid h-full min-h-0",
            "data-files-sidebar-visible": sidebars.files.to_string(),
            "data-commits-sidebar-visible": sidebars.commits.to_string(),
            if let Some(ontoggle) = ontoggle_sidebar {
                sidebars::SidebarEdge {
                    sidebar: sidebars::Sidebar::Files,
                    visible: sidebars.files,
                    ontoggle,
                }
                sidebars::SidebarEdge {
                    sidebar: sidebars::Sidebar::Commits,
                    visible: sidebars.commits,
                    ontoggle,
                }
            }
            path_filter::PathFilter { onnavigate }
            if let Some(mobile_navigation) = mobile_navigation {
                {mobile_navigation}
            }
            sidebars::SidebarPanel { sidebar: sidebars::Sidebar::Files, visible: sidebars.files,
                FilesPanel {
                    test_id: Some(test_ids::CHANGED_FILES_PANEL.value().to_owned()),
                    onnavigate,
                    sort_control: rsx! {
                        files_sort::SavedFilesSortMenu { id: "diff-files-sort" }
                    },
                    filter_control: rsx! {
                        file_filters::desktop::WorkspaceFileFiltersMenu { id: "diff-file-filters" }
                    },
                }
            }
            div { class: "diff-workspace-content min-h-0", {diff_document} }
            sidebars::SidebarPanel {
                sidebar: sidebars::Sidebar::Commits,
                visible: sidebars.commits,
                WorkspaceCommitsPanel {
                    key: "{tab_id}",
                    details_popover_id_prefix,
                    actions: commits_actions,
                    test_id: Some(test_ids::COMMITS_PANEL.value().to_owned()),
                    onselect: onselect_commit,
                    loading: commits_loading,
                    load_error: commits_error,
                    has_more: commits_has_more,
                    onloadmore: onload_commits,
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MobilePanel {
    Files,
    Commits,
}

pub(crate) mod review_actions;
