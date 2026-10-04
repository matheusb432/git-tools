use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use gtl_models::{diffs::DiffLineCount, settings::DiffFilesSort};
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::{ViewerActiveView, ViewerFileSummary};
use lucide_dioxus::{Check, ChevronRight, RotateCcw};

use super::file_filters::workspace::WorkspaceFileFilters;
use crate::{
    shared::{
        date_display::DateDisplayTime,
        i18n::{t, use_language},
        ui::{
            Button, ButtonLayout, ButtonSize, ButtonVariant, CountBadge, EmptyNotice, ScrollArea,
        },
    },
    views::diffs::{
        DiffFileStatus, DiffLineChangeBadge, DiffLineChangeKind, file_status_text_class,
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct WorkspaceLineTotals {
    added: DiffLineCount,
    removed: DiffLineCount,
}

impl WorkspaceLineTotals {
    fn from_files(files: &[ViewerFileSummary]) -> Self {
        files.iter().fold(Self::default(), |totals, file| Self {
            added: totals.added.saturating_add(file.added),
            removed: totals.removed.saturating_add(file.removed),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct WorkspaceFileTree {
    directories: Vec<(String, Self)>,
    files: Vec<usize>,
}

impl WorkspaceFileTree {
    fn from_files(files: &[ViewerFileSummary]) -> Self {
        let mut tree = Self::default();
        for (file_index, file) in files.iter().enumerate() {
            tree.insert(file.path.to_string_lossy().as_ref(), file_index);
        }
        tree
    }

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
enum WorkspaceFilesLayout {
    Tree(WorkspaceFileTree),
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkspaceFilesModel {
    totals: WorkspaceLineTotals,
    layout: WorkspaceFilesLayout,
    file_count: usize,
    commit_count: usize,
}

impl WorkspaceFilesModel {
    pub(super) fn new(view: &ViewerActiveView, sort: DiffFilesSort) -> Self {
        let layout = match sort {
            DiffFilesSort::Path => {
                WorkspaceFilesLayout::Tree(WorkspaceFileTree::from_files(&view.files))
            }
            DiffFilesSort::Changes => WorkspaceFilesLayout::List,
        };
        Self {
            totals: WorkspaceLineTotals::from_files(&view.files),
            layout,
            file_count: view.files.len(),
            commit_count: view.commit_count,
        }
    }

    pub(super) const fn file_count(&self) -> usize {
        self.file_count
    }

    pub(super) const fn commit_count(&self) -> usize {
        self.commit_count
    }
}

#[component]
pub(super) fn FilesPanel(
    test_id: Option<String>,
    onnavigate: EventHandler<String>,
    sort_control: Option<Element>,
    filter_control: Option<Element>,
) -> Element {
    let workspace = super::use_workspace_context();
    let scroll = super::panel_scroll::use_panel_scroll(super::panel_scroll::Panel::Files);
    let model = workspace.files.read();

    rsx! {
        ScrollArea {
            class: "diff-files-scroll-panel h-full min-h-0",
            "data-testid": test_id,
            onmounted: scroll.mount,
            onresize: move |_| scroll.restore.call(()),
            onscroll: scroll.save,
            div {
                class: "p-3 compact:p-2.5",
                "data-tour": super::tours::FILES_LIST.value(),
                FilesPanelHeading {
                    file_count: model.file_count,
                    sort_control,
                    filter_control,
                }
                FilesPanelSummary { totals: model.totals }
                ReviewProgress {}
                FilesHiddenNotice {}
                if model.file_count == 0 {
                    FilesEmptyNotice {}
                } else {
                    match &model.layout {
                        WorkspaceFilesLayout::Tree(tree) => render_file_tree(tree, false, onnavigate),
                        WorkspaceFilesLayout::List => rsx! {
                            ul { class: "m-0 list-none p-0",
                                for file_index in 0..model.file_count {
                                    WorkspaceFileItem {
                                        key: "{file_index}",
                                        file_index,
                                        show_directory: true,
                                        onnavigate,
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn FilesEmptyNotice() -> Element {
    let language = use_language();
    let filters = try_use_context::<super::file_filters::workspace::WorkspaceFileFilters>();
    let message = if filters.is_some_and(WorkspaceFileFilters::review_complete) {
        t!(language, "review-all-reviewed")
    } else if filters.is_some_and(|filters| (filters.active)()) {
        t!(language, "file-filters-no-matches")
    } else {
        t!(language, "files-empty")
    };
    rsx! {
        EmptyNotice { {message} }
    }
}

#[component]
fn FilesPanelHeading(
    file_count: usize,
    sort_control: Option<Element>,
    filter_control: Option<Element>,
) -> Element {
    let language = use_language();
    rsx! {
        div { class: "mx-1 mb-2 flex min-w-0 items-center justify-between gap-1",
            h2 { class: "flex min-w-0 items-center gap-1.5 font-semibold text-ink",
                span { class: "truncate", {t!(language, "workspace-files")} }
                CountBadge {
                    count: file_count,
                    aria_label: t!(language, "files-count", count = file_count),
                }
                crate::shared::ui::guided_tour::GuidedTourButton { tour: super::tours::FILES }
            }
            div { class: "flex flex-none items-center",
                span {
                    class: "inline-flex",
                    "data-tour": super::tours::FILES_SORT.value(),
                    {sort_control}
                }
                span {
                    class: "inline-flex",
                    "data-tour": super::tours::FILES_FILTER.value(),
                    {filter_control}
                }
            }
        }
    }
}

#[component]
fn FilesHiddenNotice() -> Element {
    let language = use_language();
    let Some(filters) = try_use_context::<super::file_filters::workspace::WorkspaceFileFilters>()
    else {
        return rsx! {};
    };
    if !(filters.active)() {
        return rsx! {};
    }
    let hidden_count = (filters.hidden_count)();
    let changes_since = filters.changes_since.applied.read().clone();
    rsx! {
        div { class: "diff-files-hidden-notice", role: "status",
            div { class: "diff-files-hidden-notice-text",
                if hidden_count > 0 || changes_since.is_none() {
                    span { {t!(language, "file-filters-hidden-count", count = hidden_count)} }
                }
                if let Some(changes_since) = changes_since {
                    span {
                        {t!(language, "file-filters-since-notice")}
                        " "
                        DateDisplayTime { timestamp: changes_since }
                    }
                }
            }
            Button {
                class: "flex-none",
                size: ButtonSize::IconSmall,
                variant: ButtonVariant::Ghost,
                aria_label: t!(language, "file-filters-clear"),
                title: t!(language, "file-filters-clear"),
                "data-testid": test_ids::FILE_FILTERS_CLEAR.value(),
                onclick: move |_| filters.clear.call(()),
                span { aria_hidden: "true",
                    RotateCcw { size: 14 }
                }
            }
        }
    }
}

#[component]
fn FilesPanelSummary(totals: WorkspaceLineTotals) -> Element {
    rsx! {
        div {
            class: "mx-0.5 mb-3 flex flex-wrap items-center justify-between gap-2",
            "data-tour": super::tours::FILES_TOTALS.value(),
            div { class: "flex min-w-0 flex-wrap gap-2",
                DiffLineChangeBadge {
                    kind: DiffLineChangeKind::Added,
                    count: totals.added.value(),
                }
                DiffLineChangeBadge {
                    kind: DiffLineChangeKind::Removed,
                    count: totals.removed.value(),
                }
            }
            super::titlebar::CollapseFilesButton {}
        }
    }
}

fn render_file_tree(
    tree: &WorkspaceFileTree,
    nested: bool,
    onnavigate: EventHandler<String>,
) -> Element {
    rsx! {
        ul { class: if nested { "m-0 list-none p-0 pl-2.5" } else { "m-0 list-none p-0" },
            for (directory_name, directory) in &tree.directories {
                li { class: "min-w-0",
                    details { class: "group", open: true,
                        summary { class: "diff-files-directory-summary",
                            WorkspaceDirectoryCaret {}
                            span { class: "diff-files-directory-name min-w-0", "{directory_name}" }
                        }
                        {render_file_tree(directory, true, onnavigate)}
                    }
                }
            }
            for file_index in &tree.files {
                WorkspaceFileItem {
                    file_index: *file_index,
                    show_directory: false,
                    onnavigate,
                }
            }
        }
    }
}

#[component]
fn WorkspaceDirectoryCaret() -> Element {
    rsx! {
        span {
            class: "diff-files-directory-caret group-open:rotate-90 motion-reduce:transition-none",
            aria_hidden: "true",
            ChevronRight { size: 12 }
        }
    }
}

#[component]
fn WorkspaceFileItem(
    file_index: usize,
    show_directory: bool,
    onnavigate: EventHandler<String>,
) -> Element {
    let workspace = super::use_workspace_context();
    let file = use_memo(use_reactive((&file_index,), move |(file_index,)| {
        workspace.view.read().files.get(file_index).cloned()
    }));
    let file = file.read();
    let Some(file) = file.as_ref() else {
        return rsx! {};
    };
    let file_name = file.path.as_path().file_name().map_or_else(
        || file.path.to_string_lossy(),
        |name| name.to_string_lossy(),
    );
    let directory = show_directory.then(|| {
        file.path
            .as_path()
            .parent()
            .map(|directory| directory.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let name_class = if show_directory {
        "diff-files-file-name-leading"
    } else {
        "diff-files-file-name"
    };
    let anchor_id = file.anchor_id.clone();
    let color = file_status_text_class(file.status);

    let item_attributes = merge_attributes(vec![attributes!(div {
        class: "diff-files-file-button gap-1.5 px-1.5 py-0.5 leading-snug",
    })]);

    rsx! {
        li { class: "min-w-0",
            Button {
                layout: ButtonLayout::FullWidthStart,
                size: ButtonSize::Content,
                variant: ButtonVariant::Bare,
                attributes: item_attributes,
                "data-file-target": anchor_id.clone(),

                title: file.path.to_string_lossy().into_owned(),
                onclick: move |_| onnavigate.call(anchor_id.clone()),
                if file.review.as_ref().is_some_and(|review| review.reviewed) {
                    span {
                        class: "diff-files-reviewed-mark",
                        title: t!(use_language(), "review-file-reviewed"),
                        aria_label: t!(use_language(), "review-file-reviewed"),
                        Check { size: 12 }
                    }
                }
                span { class: "{name_class} min-w-0 {color}", "{file_name}" }
                if let Some(directory) = directory {
                    span { class: "diff-files-file-directory min-w-0", "{directory}" }
                }
                DiffFileStatus { status: file.status }
            }
        }
    }
}

#[component]
fn ReviewProgress() -> Element {
    let filters = try_use_context::<super::file_filters::workspace::WorkspaceFileFilters>();
    let (reviewed, total) = filters.map_or((0, 0), |filters| (filters.review_progress)());
    if reviewed == 0 {
        return rsx! {};
    }
    rsx! {
        p {
            class: "diff-review-progress",
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            {t!(use_language(), "review-progress", reviewed = reviewed, total = total)}
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
            review: None,
            source_id: None,
            id: ViewerDiffFileId::for_index(0),
            path: repository_relative_path(path)?,
            absolute_path: absolute_file_path(format!("/repo/{path}"))?,
            anchor_id: format!("f-{}", path.replace(['/', '.'], "-")),
            added: DiffLineCount::new(added),
            removed: DiffLineCount::new(removed),
            status: ViewerFileStatus::Modified,
            can_open_in_editor: true,
            initially_expanded: true,
            row_count: 1,
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

        let tree = WorkspaceFileTree::from_files(&files);

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
