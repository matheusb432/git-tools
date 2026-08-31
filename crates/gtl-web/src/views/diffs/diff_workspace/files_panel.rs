#[cfg(feature = "desktop")]
use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use gtl_models::diffs::DiffLineCount;
#[cfg(feature = "desktop")]
use gtl_wire::viewer::ViewerViewIdentity;
#[cfg(feature = "desktop")]
use gtl_wire::viewer::{SearchViewerFiles, ViewerDiffFileId, ViewerFileSearchResult};
use gtl_wire::viewer::{ViewerActiveView, ViewerFileStatus, ViewerFileSummary};
use lucide_dioxus::ChevronRight;

#[cfg(feature = "desktop")]
use crate::{entities::diffs::viewer_server, shared::viewer_client::ViewerClientError};
use crate::{
    shared::ui::{
        Badge, Button, ButtonLayout, ButtonSize, ButtonVariant, EmptyNotice, ScrollArea, TextInput,
        TextInputLabelVisibility,
    },
    views::diffs::{
        DiffFileStatusBadge, DiffFileStatusBadgeSize, DiffLineChangeBadge, DiffLineChangeKind,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkspaceLineTotals {
    added: DiffLineCount,
    removed: DiffLineCount,
}

impl WorkspaceLineTotals {
    fn from_files(files: &[ViewerFileSummary]) -> Self {
        Self {
            added: files.iter().fold(DiffLineCount::default(), |total, file| {
                total.saturating_add(file.added)
            }),
            removed: files.iter().fold(DiffLineCount::default(), |total, file| {
                total.saturating_add(file.removed)
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct WorkspaceFileTree {
    directories: Vec<(String, Self)>,
    files: Vec<usize>,
}

impl WorkspaceFileTree {
    fn insert(&mut self, path: &str, file_index: usize) {
        let Some((directory_name, remainder)) = path.split_once('/') else {
            self.files.push(file_index);
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
        self.directories[index].1.insert(remainder, file_index);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkspaceFilesModel {
    totals: WorkspaceLineTotals,
    tree: WorkspaceFileTree,
    matching_count: usize,
    file_count: usize,
    commit_count: usize,
}

impl WorkspaceFilesModel {
    pub(super) fn new(view: &ViewerActiveView, filter: &str) -> Self {
        let filter = filter.to_lowercase();
        Self::matching(view, |file| file_path_matches(file, &filter))
    }

    #[cfg(feature = "desktop")]
    pub(super) fn from_file_ids(
        view: &ViewerActiveView,
        matches: &HashSet<ViewerDiffFileId>,
    ) -> Self {
        Self::matching(view, |file| matches.contains(&file.id))
    }

    #[cfg(feature = "desktop")]
    pub(super) fn empty(view: &ViewerActiveView) -> Self {
        Self::matching(view, |_| false)
    }

    fn matching(view: &ViewerActiveView, matches: impl Fn(&ViewerFileSummary) -> bool) -> Self {
        let mut tree = WorkspaceFileTree::default();
        let mut matching_count = 0;
        for (file_index, file) in view
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| matches(file))
        {
            let path = file.path.to_string_lossy();
            tree.insert(path.as_ref(), file_index);
            matching_count += 1;
        }
        Self {
            totals: WorkspaceLineTotals::from_files(&view.files),
            tree,
            matching_count,
            file_count: view.files.len(),
            commit_count: view.commit_count,
        }
    }

    #[cfg(feature = "artifact")]
    pub(super) const fn file_count(&self) -> usize {
        self.file_count
    }

    #[cfg(feature = "artifact")]
    pub(super) const fn commit_count(&self) -> usize {
        self.commit_count
    }
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WorkspaceFileSearch {
    Idle,
    Loading {
        identity: ViewerViewIdentity,
        query: String,
    },
    Ready {
        identity: ViewerViewIdentity,
        query: String,
        files: HashSet<ViewerDiffFileId>,
    },
    Error {
        identity: ViewerViewIdentity,
        query: String,
        error: ViewerClientError,
    },
}

#[cfg(feature = "desktop")]
impl WorkspaceFileSearch {
    pub(super) fn files_for(
        &self,
        identity: ViewerViewIdentity,
        query: &str,
    ) -> Option<&HashSet<ViewerDiffFileId>> {
        match self {
            Self::Ready {
                identity: ready_identity,
                query: ready_query,
                files,
            } if *ready_identity == identity && ready_query == query => Some(files),
            Self::Idle | Self::Loading { .. } | Self::Ready { .. } | Self::Error { .. } => None,
        }
    }

    fn status_for(
        &self,
        identity: ViewerViewIdentity,
        query: &str,
    ) -> WorkspaceFileSearchStatus<'_> {
        match self {
            Self::Loading {
                identity: request_identity,
                query: request_query,
            } if *request_identity == identity && request_query == query => {
                WorkspaceFileSearchStatus::Loading
            }
            Self::Error {
                identity: request_identity,
                query: request_query,
                error,
            } if *request_identity == identity && request_query == query => {
                WorkspaceFileSearchStatus::Error(error)
            }
            Self::Idle | Self::Loading { .. } | Self::Ready { .. } | Self::Error { .. } => {
                WorkspaceFileSearchStatus::Ready
            }
        }
    }
}

#[cfg(feature = "desktop")]
enum WorkspaceFileSearchStatus<'a> {
    Loading,
    Ready,
    Error(&'a ViewerClientError),
}

#[cfg(feature = "desktop")]
pub(super) fn use_workspace_file_search(
    view: ReadSignal<ViewerActiveView>,
    file_filter: ReadSignal<String>,
    server_owned: bool,
) -> Signal<WorkspaceFileSearch> {
    let mut search = use_signal(|| WorkspaceFileSearch::Idle);
    let _request = use_resource(move || {
        let identity = view.read().identity;
        let query = file_filter.read().clone();
        if !server_owned || query.is_empty() {
            search.set(WorkspaceFileSearch::Idle);
        } else {
            search.set(WorkspaceFileSearch::Loading {
                identity,
                query: query.clone(),
            });
        }
        request_workspace_files(view, file_filter, search, server_owned, identity, query)
    });
    search
}

#[cfg(feature = "desktop")]
async fn request_workspace_files(
    view: ReadSignal<ViewerActiveView>,
    file_filter: ReadSignal<String>,
    mut search: Signal<WorkspaceFileSearch>,
    server_owned: bool,
    identity: ViewerViewIdentity,
    query: String,
) {
    if !server_owned || query.is_empty() {
        return;
    }
    dioxus_sdk_time::sleep(std::time::Duration::from_millis(150)).await;
    if !file_search_is_current(view, file_filter, identity, &query) {
        return;
    }
    let result = viewer_server::search_files(SearchViewerFiles {
        identity,
        query: query.clone(),
    })
    .await
    .and_then(|result| validate_file_search_result(&view.peek(), identity, result));
    if !file_search_is_current(view, file_filter, identity, &query) {
        return;
    }
    match result {
        Ok(files) => search.set(WorkspaceFileSearch::Ready {
            identity,
            query,
            files,
        }),
        Err(error) => search.set(WorkspaceFileSearch::Error {
            identity,
            query,
            error,
        }),
    }
}

#[cfg(feature = "desktop")]
fn file_search_is_current(
    view: ReadSignal<ViewerActiveView>,
    file_filter: ReadSignal<String>,
    identity: ViewerViewIdentity,
    query: &str,
) -> bool {
    view.peek().identity == identity && *file_filter.peek() == query
}

#[cfg(feature = "desktop")]
fn validate_file_search_result(
    view: &ViewerActiveView,
    identity: ViewerViewIdentity,
    result: ViewerFileSearchResult,
) -> Result<HashSet<ViewerDiffFileId>, ViewerClientError> {
    if result.identity != identity {
        return Err(ViewerClientError::Internal);
    }
    let result_count = result.files.len();
    let files = result.files.into_iter().collect::<HashSet<_>>();
    if files.len() != result_count
        || files
            .iter()
            .any(|file_id| !view.files.iter().any(|file| &file.id == file_id))
    {
        return Err(ViewerClientError::Internal);
    }
    Ok(files)
}

fn file_path_matches(file: &ViewerFileSummary, filter: &str) -> bool {
    file.path.to_string_lossy().to_lowercase().contains(filter)
}

#[component]
pub(super) fn FilesPanel(
    test_id: Option<String>,
    onnavigate: EventHandler<String>,
    artifact_view_id: Option<String>,
) -> Element {
    let workspace = super::use_workspace_context();
    let model = workspace.files.read();
    let artifact_enhancement = artifact_view_id.is_some();
    let artifact_file_panel = artifact_enhancement.then_some("");
    #[cfg(feature = "desktop")]
    let (searching, search_error) = if artifact_enhancement {
        (false, None)
    } else {
        let identity = workspace.view.peek().identity;
        let filter = workspace.file_filter.peek();
        match workspace.file_search.read().status_for(identity, &filter) {
            WorkspaceFileSearchStatus::Loading => (true, None),
            WorkspaceFileSearchStatus::Ready => (false, None),
            WorkspaceFileSearchStatus::Error(error) => (false, Some(error.message().to_owned())),
        }
    };
    #[cfg(not(feature = "desktop"))]
    let (searching, search_error) = (false, None::<String>);

    rsx! {
        ScrollArea {
            class: "h-full min-h-0 overflow-auto bg-surface p-3 compact:p-2.5",
            "data-testid": test_id,
            "data-gtl-file-panel": artifact_file_panel,
            FilesFilter { artifact_enhancement }
            FilesPanelHeading { file_count: model.file_count }
            FilesPanelSummary { commit_count: model.commit_count, totals: model.totals }
            if searching {
                p { class: "px-1 py-3 text-ink-3", role: "status", "Searching files\u{2026}" }
            } else if let Some(error) = search_error {
                p { class: "px-1 py-3 text-del", role: "alert", "{error}" }
            } else if artifact_enhancement {
                {render_file_tree(&model.tree, false, onnavigate, true)}
                EmptyNotice {
                    hidden: model.matching_count > 0,
                    "data-gtl-files-empty": "",
                    "no files match this filter"
                }
            } else if model.matching_count == 0 {
                EmptyNotice { "no files match this filter" }
            } else {
                {render_file_tree(&model.tree, false, onnavigate, false)}
            }
        }
    }
}

#[component]
fn FilesFilter(artifact_enhancement: bool) -> Element {
    let artifact_action = artifact_enhancement.then_some("filter-files");
    let mut workspace = super::use_workspace_context();
    rsx! {
        div { class: "relative mb-3",
            TextInput {
                label: "Filter files",
                label_visibility: TextInputLabelVisibility::Hidden,
                class: "h-9 py-2",
                value: (workspace.file_filter)(),
                placeholder: "Filter files\u{2026}  /",
                "data-gtl-action": artifact_action,
                oninput: move |event: FormEvent| workspace.file_filter.set(event.value()),
            }
        }
    }
}

#[component]
fn FilesPanelHeading(file_count: usize) -> Element {
    let file_label = super::file_label(file_count);
    rsx! {
        div { class: "mx-1 mt-1.5 mb-2 flex justify-between tracking-wider text-ink-3 uppercase",
            span { "# {file_count} {file_label}" }
        }
    }
}

#[component]
fn FilesPanelSummary(commit_count: usize, totals: WorkspaceLineTotals) -> Element {
    rsx! {
        div { class: "mx-0.5 mb-3 flex flex-wrap gap-2",
            CommitCountBadge { count: commit_count }
            DiffLineChangeBadge { kind: DiffLineChangeKind::Added, count: totals.added.value() }
            DiffLineChangeBadge {
                kind: DiffLineChangeKind::Removed,
                count: totals.removed.value(),
            }
        }
    }
}

#[component]
fn CommitCountBadge(count: usize) -> Element {
    let commit_label = super::commit_label(count);
    rsx! {
        Badge { class: "px-2 py-0.5",
            b { class: "font-bold text-ink", "{count}" }
            span { class: "ml-1", {commit_label} }
        }
    }
}

fn render_file_tree(
    tree: &WorkspaceFileTree,
    nested: bool,
    onnavigate: EventHandler<String>,
    artifact_enhancement: bool,
) -> Element {
    let artifact_tree = (!nested && artifact_enhancement).then_some("");
    rsx! {
        ul {
            class: if nested { "m-0 list-none p-0 pl-2.5" } else { "m-0 list-none p-0" },
            "data-gtl-file-tree": artifact_tree,
            for (directory_name, directory) in &tree.directories {
                li {
                    class: "min-w-0",
                    "data-gtl-file-directory": artifact_enhancement.then_some(""),
                    details { class: "group", open: true,
                        summary { class: "flex cursor-pointer list-none items-center gap-1.5 rounded-sm px-1.5 py-0.5 leading-snug text-ink-3 hover:bg-surface-2 hover:text-ink active:bg-acc-soft focus-visible:-outline-offset-2 focus-visible:outline-2 focus-visible:outline-acc [&::-webkit-details-marker]:hidden",
                            WorkspaceDirectoryCaret {}
                            span { class: "min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap",
                                "{directory_name}"
                            }
                        }
                        {render_file_tree(directory, true, onnavigate, artifact_enhancement)}
                    }
                }
            }
            for file_index in &tree.files {
                WorkspaceFileItem {
                    file_index: *file_index,
                    onnavigate,
                    artifact_enhancement,
                }
            }
        }
    }
}

#[component]
fn WorkspaceDirectoryCaret() -> Element {
    rsx! {
        span {
            class: "flex-none transition-transform group-open:rotate-90 motion-reduce:transition-none",
            aria_hidden: "true",
            ChevronRight { size: 12 }
        }
    }
}

#[component]
fn WorkspaceFileItem(
    file_index: usize,
    onnavigate: EventHandler<String>,
    artifact_enhancement: bool,
) -> Element {
    let workspace = super::use_workspace_context();
    let _data_generation = (workspace.data_generation)();
    let view = workspace.view.peek();
    let Some(file) = view.files.get(file_index) else {
        return rsx! {};
    };
    let file_name = file.path.as_path().file_name().map_or_else(
        || file.path.to_string_lossy(),
        |name| name.to_string_lossy(),
    );
    let anchor_id = file.anchor_id.clone();
    let filter_key = file.path.to_string_lossy().to_lowercase();
    let tone_classes = file_item_tone_classes(file.status);
    let artifact_file = artifact_enhancement.then_some("");
    let artifact_action = artifact_enhancement.then_some("navigate-file");
    let item_attributes = merge_attributes(vec![
        attributes!(div {
            class: "gap-1.5 px-1.5 py-0.5 text-left leading-snug text-ink-2 hover:text-ink",
        }),
        attributes!(div {
            class: tone_classes,
        }),
    ]);

    rsx! {
        li {
            class: "min-w-0",
            "data-gtl-file-leaf": artifact_file,
            "data-gtl-filter-key": artifact_enhancement.then_some(filter_key),
            Button {
                layout: ButtonLayout::FullWidthStart,
                size: ButtonSize::Content,
                variant: ButtonVariant::Bare,
                attributes: item_attributes,
                "data-file-target": anchor_id.clone(),
                "data-gtl-action": artifact_action,
                title: file.path.to_string_lossy().into_owned(),
                onclick: move |_| onnavigate.call(anchor_id.clone()),
                DiffFileStatusBadge {
                    status: file.status,
                    size: DiffFileStatusBadgeSize::Compact,
                }
                span { class: "min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap",
                    "{file_name}"
                }
            }
        }
    }
}

const fn file_item_tone_classes(status: ViewerFileStatus) -> &'static str {
    match status {
        ViewerFileStatus::Added => "bg-add-bg/40 hover:bg-add-bg/60 active:bg-add-bg",
        ViewerFileStatus::Deleted => "bg-del-bg/40 hover:bg-del-bg/60 active:bg-del-bg",
        ViewerFileStatus::Renamed | ViewerFileStatus::Modified => {
            "bg-transparent hover:bg-surface-2 active:bg-acc-soft"
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::diffs::DiffLineCount;
    use gtl_wire::viewer::{ViewerDiffFileId, ViewerFileStatus, ViewerFileSummary};

    use super::{WorkspaceFileTree, WorkspaceLineTotals};
    use crate::test_support::{TestResult, absolute_file_path, repository_relative_path};

    fn file(path: &str, added: u64, removed: u64) -> TestResult<ViewerFileSummary> {
        Ok(ViewerFileSummary {
            id: ViewerDiffFileId::for_index(0),
            path: repository_relative_path(path)?,
            absolute_path: absolute_file_path(format!("/repo/{path}"))?,
            anchor_id: format!("f-{}", path.replace(['/', '.'], "-")),
            added: DiffLineCount::new(added),
            removed: DiffLineCount::new(removed),
            status: ViewerFileStatus::Modified,
            can_open_in_editor: true,
            initially_expanded: true,
        })
    }

    #[test]
    fn changed_files_summary_retains_total_line_changes() -> TestResult {
        let files = [file("src/added.rs", 3, 1)?, file("src/removed.rs", 1, 5)?];

        assert_eq!(
            WorkspaceLineTotals::from_files(&files),
            WorkspaceLineTotals {
                added: DiffLineCount::new(4),
                removed: DiffLineCount::new(6),
            }
        );
        Ok(())
    }

    #[test]
    fn changed_files_tree_preserves_path_hierarchy() -> TestResult {
        let files = [
            file("crates/web/src/app.rs", 3, 1)?,
            file("crates/web/src/view.rs", 1, 5)?,
        ];

        let mut tree = WorkspaceFileTree::default();
        for (index, file) in files.iter().enumerate() {
            tree.insert(file.path.to_string_lossy().as_ref(), index);
        }

        assert_eq!(tree.directories[0].0, "crates");
        assert_eq!(tree.directories[0].1.directories[0].0, "web");
        assert_eq!(
            tree.directories[0].1.directories[0].1.directories[0]
                .1
                .files[0],
            0
        );
        Ok(())
    }
}
