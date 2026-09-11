use dioxus::prelude::*;
use gtl_models::diffs::CommitId;
#[cfg(feature = "component-preview")]
use gtl_models::viewer::ViewerKeybindingAction;
#[cfg(feature = "component-preview")]
use gtl_models::viewer::ViewerKeybindings;
#[cfg(any(feature = "artifact", feature = "desktop"))]
use gtl_models::viewer::ViewerTabId;
use gtl_web_contracts::test_ids;
#[cfg(feature = "component-preview")]
use gtl_wire::viewer::ViewerCommitSelection;
#[cfg(feature = "artifact")]
use gtl_wire::viewer::ViewerDiffFileId;
use gtl_wire::viewer::{ViewerActiveView, ViewerCommitSummary};
#[cfg(any(
    feature = "artifact",
    feature = "component-preview",
    feature = "desktop"
))]
use lucide_dioxus::{Files, GitCommitHorizontal};

#[cfg(feature = "component-preview")]
use self::titlebar::PreviewViewTitlebar;
use self::{
    commits_panel::WorkspaceCommitsPanel,
    file_search::{WorkspaceFileMatches, use_workspace_file_matches},
    files_panel::{FilesPanel, WorkspaceFilesModel},
    titlebar::ViewTitlebar,
};
#[cfg(feature = "component-preview")]
use super::{
    client_diff_document::search_bar::{DiffSearchBar, DiffSearchScope},
    search_keybindings::keyboard_event_matches,
};
#[cfg(feature = "component-preview")]
use crate::shared::browser;
#[cfg(feature = "artifact")]
use crate::shared::ui::FloatingNotice;
#[cfg(any(feature = "artifact", feature = "component-preview"))]
use crate::shared::ui::PanelDialog;
#[cfg(any(
    feature = "artifact",
    feature = "component-preview",
    feature = "desktop"
))]
use crate::shared::ui::{Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant, CountBadge};
#[cfg(feature = "artifact")]
use crate::{entities::diffs::ClientDiffWorkspace, views::diffs::StaticDiffDocument};

pub(crate) mod commits_panel;
#[cfg(feature = "desktop")]
mod desktop;
mod file_search;
mod files_panel;
mod path_filter;
mod sidebars;
mod titlebar;

#[cfg(feature = "desktop")]
pub(crate) use desktop::DiffWorkspaceView;

#[cfg(feature = "desktop")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct FileFoldCommand {
    pub(super) tab_id: ViewerTabId,
    pub(super) folded: bool,
}

#[derive(Clone, Copy)]
pub(super) struct DiffWorkspaceContext {
    pub(super) view: ReadSignal<ViewerActiveView>,
    commits: ReadStore<Vec<ViewerCommitSummary>>,
    files: Memo<WorkspaceFilesModel>,
    file_filter: Signal<String>,
    path_filter_open: Signal<bool>,
    file_matches: Memo<WorkspaceFileMatches>,
    pub(super) files_folded: Signal<Option<bool>>,
    #[cfg(feature = "desktop")]
    pub(super) fold_command: Signal<Option<FileFoldCommand>>,
    pub(super) flashing_file: Signal<Option<String>>,
    #[cfg(feature = "desktop")]
    pub(super) find_open: Signal<bool>,
}

#[derive(Clone, Copy)]
struct DiffWorkspaceSignals {
    file_filter: Signal<String>,
    path_filter_open: Signal<bool>,
    files_folded: Signal<Option<bool>>,
    flashing_file: Signal<Option<String>>,
    #[cfg(feature = "desktop")]
    find_open: Signal<bool>,
}

fn use_diff_workspace_context(
    view: ReadSignal<ViewerActiveView>,
    commits: ReadStore<Vec<ViewerCommitSummary>>,
    signals: DiffWorkspaceSignals,
    server_owned_file_search: bool,
) -> DiffWorkspaceContext {
    #[cfg(not(feature = "desktop"))]
    let _ = server_owned_file_search;
    let DiffWorkspaceSignals {
        file_filter,
        path_filter_open,
        files_folded,
        flashing_file,
        #[cfg(feature = "desktop")]
        find_open,
    } = signals;
    let file_matches =
        use_workspace_file_matches(view, file_filter.into(), server_owned_file_search);
    let files = use_memo(move || WorkspaceFilesModel::new(&view.read()));
    let context = DiffWorkspaceContext {
        view,
        commits,
        files,
        file_filter,
        path_filter_open,
        file_matches,
        files_folded,
        #[cfg(feature = "desktop")]
        fold_command: use_signal(|| None),
        flashing_file,
        #[cfg(feature = "desktop")]
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
        file_filter: use_signal(String::new),
        path_filter_open: use_signal(move || path_filter_open_initial),
        files_folded: use_signal(|| None::<bool>),
        flashing_file: use_signal(|| None::<String>),
        #[cfg(feature = "desktop")]
        find_open: use_signal(|| false),
    };
    use_diff_workspace_context(view, commits, signals, false)
}

#[cfg(any(feature = "desktop", feature = "component-preview"))]
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

#[cfg(any(
    feature = "artifact",
    feature = "component-preview",
    feature = "desktop"
))]
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
    #[props(default)] artifact: bool,
    #[props(default)] preview_visible: bool,
    onfiles: EventHandler<MouseEvent>,
    oncommits: EventHandler<MouseEvent>,
) -> Element {
    let navigation_classes = if preview_visible {
        "diff-workspace-mobile-navigation"
    } else {
        "diff-workspace-mobile-navigation workspace:hidden"
    };
    let artifact_action = artifact.then_some("open-dialog");

    rsx! {
        nav { class: navigation_classes, aria_label: "Viewer panels",
            Button {
                id: files_trigger_id,
                class: "flex min-h-11 min-w-0 justify-center gap-2 rounded-none border-0 border-r px-3 focus-visible:-outline-offset-2",
                layout: ButtonLayout::Content,
                size: ButtonSize::Content,
                variant: ButtonVariant::Ghost,
                state: if file_count > 0 { ButtonState::Enabled } else { ButtonState::Disabled },
                aria_label: "Changed files",
                aria_controls: files_panel_id,
                aria_expanded: files_open.to_string(),
                aria_haspopup: "dialog",
                "data-gtl-action": artifact_action,
                onclick: onfiles,
                span {
                    class: "inline-flex size-5 flex-none items-center justify-center [&_svg]:size-5",
                    aria_hidden: "true",
                    Files { size: 20 }
                }
                span { class: "font-semibold", "Files" }
                CountBadge { count: file_count }
            }
            Button {
                id: commits_trigger_id,
                class: "flex min-h-11 min-w-0 justify-center gap-2 rounded-none border-0 px-3 focus-visible:-outline-offset-2",
                layout: ButtonLayout::Content,
                size: ButtonSize::Content,
                variant: ButtonVariant::Ghost,
                state: if commit_count > 0 { ButtonState::Enabled } else { ButtonState::Disabled },
                aria_label: "Commits",
                aria_controls: commits_panel_id,
                aria_expanded: commits_open.to_string(),
                aria_haspopup: "dialog",
                "data-gtl-action": artifact_action,
                onclick: oncommits,
                span {
                    class: "inline-flex size-5 flex-none items-center justify-center [&_svg]:size-5",
                    aria_hidden: "true",
                    GitCommitHorizontal { size: 20 }
                }
                span { class: "font-semibold", "Commits" }
                CountBadge { count: commit_count }
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
    workspace: ClientDiffWorkspace,
    #[props(default)] mobile: bool,
    #[props(default)] initial_search: PreviewDiffSearch,
    keybindings: ViewerKeybindings,
) -> Element {
    let markup = ArtifactViewMarkup::new(view.identity.tab_id);
    for file in &mut view.files {
        file.anchor_id = markup.file_target_id(&file.id);
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
    let onclear_commit = use_callback(move |()| {
        view.write().commit_selection = ViewerCommitSelection::None;
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
                PreviewViewTitlebar { mobile: true, onfindall: open_all_files_search }
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
                    StaticDiffDocument { workspace: workspace.clone(), overlay: search_overlay }
                }
            }
            PanelDialog {
                id: "preview-mobile-files-panel",
                trigger_id: "preview-mobile-files-trigger",
                open: mobile_panel() == Some(PreviewMobilePanel::Files),
                title: "Changed files",
                onclose: move |()| mobile_panel.set(None),
                FilesPanel { onnavigate }
            }
            PanelDialog {
                id: "preview-mobile-commits-panel",
                trigger_id: "preview-mobile-commits-trigger",
                open: mobile_panel() == Some(PreviewMobilePanel::Commits),
                title: "Commits",
                onclose: move |()| mobile_panel.set(None),
                WorkspaceCommitsPanel {
                    details_popover_id_prefix: "preview-mobile-commits-panel",
                    onselect: onselect_commit,
                    onclear: onclear_commit,
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
                PreviewViewTitlebar { onfindall: open_all_files_search }
                aside {
                    class: "diff-workspace-panel min-h-0 diff-workspace-files-panel",
                    aria_label: "Changed files",
                    FilesPanel { onnavigate }
                }
                StaticDiffDocument { workspace, overlay: search_overlay }
                aside {
                    class: "diff-workspace-panel min-h-0 diff-workspace-commits-panel",
                    aria_label: "Commits",
                    WorkspaceCommitsPanel {
                        details_popover_id_prefix: "preview-desktop-commits-panel",
                        onselect: onselect_commit,
                        onclear: onclear_commit,
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

#[cfg(feature = "artifact")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArtifactViewMarkup {
    tab_id: ViewerTabId,
}

#[cfg(feature = "artifact")]
impl ArtifactViewMarkup {
    fn new(tab_id: ViewerTabId) -> Self {
        Self { tab_id }
    }

    fn view_id(self) -> String {
        self.tab_id.to_string()
    }

    fn target_prefix(self) -> String {
        format!("artifact-view-{}", self.tab_id)
    }

    fn control_id(self, control: &str) -> String {
        format!("{}-{control}", self.target_prefix())
    }

    fn file_target_id(self, file_id: &ViewerDiffFileId) -> String {
        format!("{}-{}", self.target_prefix(), file_id.as_str())
    }
}

#[cfg(feature = "artifact")]
#[component]
pub(crate) fn ArtifactDiffWorkspace(
    mut view: ViewerActiveView,
    mut workspace: ClientDiffWorkspace,
) -> Element {
    let markup = ArtifactViewMarkup::new(view.identity.tab_id);
    for file in &mut view.files {
        file.anchor_id = markup.file_target_id(&file.id);
    }
    for file in &mut workspace.files {
        file.summary.anchor_id = markup.file_target_id(&file.summary.id);
    }
    let commits = std::mem::take(&mut view.commits);
    let view = use_signal(move || view);
    let commits = use_store(move || commits);
    let context = use_static_diff_workspace_context(view.into(), commits.into(), false);
    let (file_count, commit_count) = context
        .files
        .with(|files| (files.file_count(), files.commit_count()));

    let files_trigger = markup.control_id("files-trigger");
    let files_dialog = markup.control_id("files-dialog");
    let commits_trigger = markup.control_id("commits-trigger");
    let commits_dialog = markup.control_id("commits-dialog");
    let mobile_navigation = rsx! {
        WorkspaceMobileNavigation {
            files_trigger_id: files_trigger.clone(),
            files_panel_id: files_dialog.clone(),
            commits_trigger_id: commits_trigger.clone(),
            commits_panel_id: commits_dialog.clone(),
            file_count,
            commit_count,
            artifact: true,
            onfiles: move |_| {},
            oncommits: move |_| {},
        }
    };
    let diff_document = rsx! {
        StaticDiffDocument { workspace }
    };

    rsx! {
        section { class: "h-full min-h-0 overflow-hidden",
            DiffWorkspaceDocument {
                diff_document,
                onnavigate: move |_| {},
                mobile_navigation,
                artifact_view_id: Some(markup.view_id()),
            }
        }
        FloatingNotice {
            hidden: true,
            role: "status",
            aria_live: "polite",
            "data-gtl-copy-context-feedback": "",
        }

        PanelDialog {
            id: files_dialog,
            trigger_id: files_trigger,
            open: false,
            title: "Changed files",
            onclose: move |()| {},
            artifact_view_id: Some(markup.view_id()),
            FilesPanel {
                onnavigate: move |_| {},
                artifact_view_id: Some(markup.view_id()),
            }
        }
        PanelDialog {
            id: commits_dialog.clone(),
            trigger_id: commits_trigger,
            open: false,
            title: "Commits",
            onclose: move |()| {},
            artifact_view_id: Some(markup.view_id()),
            WorkspaceCommitsPanel { details_popover_id_prefix: commits_dialog, artifact: true }
        }
    }
}

#[component]
fn DiffWorkspaceDocument(
    #[props(default)] sidebars: gtl_models::viewer::ViewerSidebarVisibility,
    #[props(default)] sidebar_pending: bool,
    #[props(default)] keybindings: gtl_models::viewer::ViewerKeybindings,
    ontoggle_sidebar: Option<EventHandler<sidebars::Sidebar>>,
    diff_document: Element,
    onnavigate: EventHandler<String>,
    mobile_navigation: Option<Element>,
    live_actions: Option<Element>,
    onselect_commit: Option<EventHandler<CommitId>>,
    onclear_commit: Option<EventHandler<()>>,
    #[props(default)] commits_loading: bool,
    commits_error: Option<String>,
    #[props(default)] commits_has_more: bool,
    onload_commits: Option<EventHandler<()>>,
    artifact_view_id: Option<String>,
) -> Element {
    let workspace = use_workspace_context();
    let files_folded = (workspace.files_folded)();
    let artifact_workspace = artifact_view_id.as_ref().map(|_| "");
    let artifact_files_folded = artifact_view_id
        .as_ref()
        .map(|_| files_folded.unwrap_or(false).to_string());
    let details_popover_id_prefix = artifact_view_id.as_ref().map_or_else(
        || "workspace-commits-panel".to_owned(),
        |view_id| format!("{view_id}-workspace-commits-panel"),
    );
    rsx! {
        div {
            class: "diff-workspace-grid h-full min-h-0",
            "data-gtl-workspace": artifact_workspace,
            "data-files-sidebar-visible": sidebars.files.to_string(),
            "data-commits-sidebar-visible": sidebars.commits.to_string(),
            "data-gtl-view": artifact_view_id.clone(),
            "data-gtl-files-folded": artifact_files_folded,
            path_filter::PathFilter { onnavigate, artifact_view_id: artifact_view_id.clone() }
            ViewTitlebar {
                live_actions,
                artifact_view_id: artifact_view_id.clone(),
                sidebars,
                sidebar_pending,
                keybindings,
                ontoggle_sidebar,
                onclear_commit,
            }
            if let Some(mobile_navigation) = mobile_navigation {
                {mobile_navigation}
            }
            sidebars::SidebarPanel { sidebar: sidebars::Sidebar::Files, visible: sidebars.files,
                FilesPanel {
                    test_id: Some(test_ids::CHANGED_FILES_PANEL.value().to_owned()),
                    onnavigate,
                    artifact_view_id: artifact_view_id.clone(),
                }
            }
            div { class: "diff-workspace-mobile-content min-h-0 workspace:col-start-2 workspace:col-span-1 workspace:row-start-2",
                {diff_document}
            }
            sidebars::SidebarPanel {
                sidebar: sidebars::Sidebar::Commits,
                visible: sidebars.commits,
                WorkspaceCommitsPanel {
                    details_popover_id_prefix,
                    artifact: artifact_view_id.is_some(),
                    test_id: Some(test_ids::COMMITS_PANEL.value().to_owned()),
                    onselect: onselect_commit,
                    onclear: onclear_commit,
                    loading: commits_loading,
                    load_error: commits_error,
                    has_more: commits_has_more,
                    onloadmore: onload_commits,
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MobilePanel {
    Files,
    Commits,
}

// TODO: refactor these to cleaner, intl compatible shape
const fn file_label(count: usize) -> &'static str {
    if count == 1 { "file" } else { "files" }
}

const fn commit_label(count: usize) -> &'static str {
    if count == 1 { "commit" } else { "commits" }
}

#[cfg(all(test, feature = "artifact"))]
mod artifact_tests {
    use dioxus::prelude::*;
    use gtl_models::{
        diffs::{CommitId, DiffLineCount},
        git::{BranchName, GitHead, GitRevision},
        viewer::{ViewerRangeGeneration, ViewerSelectionGeneration},
    };
    use gtl_wire::viewer::{
        ViewerActiveView, ViewerCommandLine, ViewerCommitSelection, ViewerCommitSummary,
        ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout, ViewerFileRows, ViewerFileStatus,
        ViewerFileSummary, ViewerFooter, ViewerRenderOptions, ViewerRows, ViewerViewIdentity,
    };

    use super::{ArtifactDiffWorkspace, ArtifactViewMarkup};
    use crate::{
        entities::diffs::{ClientDiffWorkspace, static_diff_workspace},
        test_support::{
            TestResult, absolute_file_path, machine_timestamp, project_name,
            repository_relative_path, viewer_tab_id,
        },
    };

    fn artifact_view(tab_id: u64) -> TestResult<(ViewerActiveView, ClientDiffWorkspace)> {
        let identity = ViewerViewIdentity {
            tab_id: viewer_tab_id(tab_id)?,
            range_generation: ViewerRangeGeneration::new(3),
            selection_generation: ViewerSelectionGeneration::new(5),
            render_options: ViewerRenderOptions {
                wrap_lines: false,
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        };
        let file = ViewerFileSummary {
            id: ViewerDiffFileId::for_index(0),
            path: repository_relative_path("src/<unsafe>.rs")?,
            absolute_path: absolute_file_path("/repo/src/<unsafe>.rs")?,
            anchor_id: "file-0".to_owned(),
            added: DiffLineCount::new(1),
            removed: DiffLineCount::new(1),
            status: ViewerFileStatus::Modified,
            can_open_in_editor: true,
            initially_expanded: true,
            row_count: 1,
        };
        let workspace = static_diff_workspace(
            identity,
            vec![(
                file.clone(),
                ViewerFileRows {
                    rows: ViewerRows::Unified(Vec::new()),
                    line_number_digits: 1,
                },
            )],
        );
        let view = ViewerActiveView {
            row_source: gtl_wire::viewer::ViewerRowSourceState::Ready,
            content_id: gtl_wire::viewer::ViewerRowContentId::from_digest([0; 32]),
            identity,
            title: "diff".to_owned(),
            repository_name: project_name(&format!("repo-{tab_id}"))?,
            branch: GitHead::Branch(BranchName::main()),
            upstream: GitRevision::main(),
            command: ViewerCommandLine {
                lead: "git diff ".to_owned(),
                range: "HEAD~1..HEAD".to_owned(),
                trail: String::new(),
            },
            files: vec![file],
            commits_label: "1 commit".to_owned(),
            commit_count: 1,
            commits: vec![ViewerCommitSummary {
                id: CommitId::try_from("0123456789abcdef0123456789abcdef01234567")?,
                subject: "static render".to_owned(),
                body: String::new(),
                committed_at: machine_timestamp("2026-08-19T10:00:00Z")?,
                is_merge: false,
            }],
            commit_selection: ViewerCommitSelection::None,
            footer: ViewerFooter {
                command: "gtl diff".to_owned(),
            },
            exclusions: None,
        };
        Ok((view, workspace))
    }

    #[test]
    fn artifact_workspace_renders_scoped_enhancement_contract() -> TestResult {
        let (view, workspace) = artifact_view(7)?;
        let html = dioxus_ssr::render_element(rsx! {
            ArtifactDiffWorkspace { view, workspace }
        });

        assert!(html.contains(r#"data-gtl-workspace="""#));
        assert!(html.contains(r#"data-gtl-view="7""#));
        assert!(html.contains(r#"data-gtl-files-folded="false""#));
        assert_copy_context_feedback_markup(&html);
        assert_eq!(html.matches(r#"data-gtl-action="filter-files""#).count(), 1);
        assert_eq!(html.matches(r#"data-gtl-action="toggle-files""#).count(), 1);
        assert_eq!(html.matches(r#"data-gtl-action="open-dialog""#).count(), 2);
        assert!(!html.contains("toggle-copy-context"));
        assert!(!html.contains("+ context"));
        assert!(!html.contains("<footer"));
        assert!(!html.contains("gtl diff"));

        assert_eq!(
            html.matches(r#"data-file-target="artifact-view-7-file-0""#)
                .count(),
            3
        );
        assert!(html.contains(r#"id="artifact-view-7-file-0""#));
        assert!(html.contains(r#"id="artifact-view-7-files-trigger""#));
        assert!(html.contains(r#"aria-controls="artifact-view-7-files-dialog""#));
        assert!(html.contains(r#"id="artifact-view-7-files-dialog""#));
        assert!(html.contains("inline-flex size-5 flex-none items-center justify-center"));
        assert!(html.contains(r#"data-gtl-dialog-trigger="artifact-view-7-files-trigger""#,));
        assert!(html.contains(r#"data-gtl-action="close-dialog""#));
        assert!(html.contains(r#"data-gtl-action="copy-commit""#));
        assert!(
            html.contains(r#"data-gtl-copy-value="0123456789abcdef0123456789abcdef01234567""#,)
        );
        assert!(!html.contains(r#"id="src/<unsafe>.rs""#));
        Ok(())
    }

    fn assert_copy_context_feedback_markup(html: &str) {
        assert!(html.contains(r#"data-gtl-copy-context-feedback="""#));
        assert!(html.contains(r#"aria-live="polite""#));
    }

    #[test]
    fn artifact_targets_are_prefixed_by_typed_tab_ids() -> TestResult {
        let first = ArtifactViewMarkup::new(viewer_tab_id(1)?);
        let second = ArtifactViewMarkup::new(viewer_tab_id(2)?);
        let file = ViewerDiffFileId::for_index(0);

        assert_ne!(
            first.control_id("files-dialog"),
            second.control_id("files-dialog")
        );
        assert_ne!(first.file_target_id(&file), second.file_target_id(&file));
        assert_eq!(first.file_target_id(&file), "artifact-view-1-file-0");
        Ok(())
    }
}
