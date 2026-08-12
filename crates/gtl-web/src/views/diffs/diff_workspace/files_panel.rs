use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use gtl_wire::viewer::{ViewerActiveView, ViewerFileStatus, ViewerFileSummary};
use lucide_dioxus::ChevronRight;

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
pub(super) fn FilesPanel(
    view: ViewerActiveView,
    filter: String,
    onfilter: EventHandler<String>,
    onnavigate: EventHandler<String>,
) -> Element {
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
            FilesFilter { filter, onfilter }
            FilesPanelHeading {
                commits_label: view.commits_label.clone(),
                file_count: view.files.len(),
            }
            FilesPanelSummary { commit_count: view.commits.len(), totals }
            if files.is_empty() {
                EmptyNotice { "no files match this filter" }
            } else {
                WorkspaceFileTreeView { tree, onnavigate }
            }
        }
    }
}

#[component]
fn FilesFilter(filter: String, onfilter: EventHandler<String>) -> Element {
    rsx! {
        div { class: "relative mb-3",
            TextInput {
                label: "Filter files",
                label_visibility: TextInputLabelVisibility::Hidden,
                class: "h-9 py-2",
                value: filter,
                placeholder: "Filter files\u{2026}  /",
                oninput: move |event: FormEvent| onfilter.call(event.value()),
            }
        }
    }
}

#[component]
fn FilesPanelHeading(commits_label: String, file_count: usize) -> Element {
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
            DiffLineChangeBadge { kind: DiffLineChangeKind::Added, count: totals.added }
            DiffLineChangeBadge { kind: DiffLineChangeKind::Removed, count: totals.removed }
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

#[component]
fn WorkspaceFileTreeView(
    tree: WorkspaceFileTree,
    onnavigate: EventHandler<String>,
    #[props(default)] nested: bool,
) -> Element {
    rsx! {
        ul { class: if nested { "m-0 list-none p-0 pl-2.5" } else { "m-0 list-none p-0" },
            for (directory_name, directory) in tree.directories {
                WorkspaceDirectoryItem { directory_name, directory, onnavigate }
            }
            for (file_name, file) in tree.files {
                WorkspaceFileItem { file_name, file, onnavigate }
            }
        }
    }
}

#[component]
fn WorkspaceDirectoryItem(
    directory_name: String,
    directory: WorkspaceFileTree,
    onnavigate: EventHandler<String>,
) -> Element {
    rsx! {
        li { class: "min-w-0",
            details { class: "group", open: true,
                summary { class: "flex cursor-pointer list-none items-center gap-1.5 rounded-sm px-1.5 py-0.5 leading-snug text-ink-3 hover:bg-surface-2 hover:text-ink active:bg-acc-soft focus-visible:-outline-offset-2 focus-visible:outline-2 focus-visible:outline-acc [&::-webkit-details-marker]:hidden",
                    WorkspaceDirectoryCaret {}
                    WorkspaceTreeLabel { label: directory_name }
                }
                WorkspaceFileTreeView { tree: directory, onnavigate, nested: true }
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
    file_name: String,
    file: ViewerFileSummary,
    onnavigate: EventHandler<String>,
) -> Element {
    let anchor_id = file.anchor_id.clone();
    let tone_classes = file_item_tone_classes(file.status);
    let item_attributes = merge_attributes(vec![
        attributes!(div {
            class: "gap-1.5 px-1.5 py-0.5 text-left leading-snug text-ink-2 hover:text-ink",
        }),
        attributes!(div {
            class: tone_classes,
        }),
    ]);

    rsx! {
        li { class: "min-w-0",
            Button {
                layout: ButtonLayout::FullWidthStart,
                size: ButtonSize::Content,
                variant: ButtonVariant::Bare,
                attributes: item_attributes,
                "data-file-target": anchor_id.clone(),
                title: file.path,
                onclick: move |_| onnavigate.call(anchor_id.clone()),
                DiffFileStatusBadge {
                    status: file.status,
                    size: DiffFileStatusBadgeSize::Compact,
                }
                WorkspaceTreeLabel { label: file_name }
            }
        }
    }
}

#[component]
fn WorkspaceTreeLabel(label: String) -> Element {
    rsx! {
        span { class: "min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap",
            "{label}"
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
    use gtl_wire::viewer::{ViewerDiffFileId, ViewerFileStatus, ViewerFileSummary};

    use super::{WorkspaceFileTree, WorkspaceLineTotals};

    fn file(path: &str, added: u32, removed: u32) -> ViewerFileSummary {
        ViewerFileSummary {
            id: ViewerDiffFileId::for_index(0),
            path: path.to_owned(),
            absolute_path: format!("/repo/{path}"),
            anchor_id: format!("f-{}", path.replace(['/', '.'], "-")),
            added,
            removed,
            status: ViewerFileStatus::Modified,
            can_open_in_editor: true,
            initially_expanded: true,
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
