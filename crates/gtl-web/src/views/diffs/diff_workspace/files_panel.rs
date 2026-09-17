use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use gtl_models::diffs::DiffLineCount;
use gtl_wire::viewer::{ViewerActiveView, ViewerFileSummary};
use lucide_dioxus::ChevronRight;

use crate::{
    shared::ui::{Button, ButtonLayout, ButtonSize, ButtonVariant, EmptyNotice, ScrollArea},
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
    file_count: usize,
    commit_count: usize,
}

impl WorkspaceFilesModel {
    pub(super) fn new(view: &ViewerActiveView) -> Self {
        let mut tree = WorkspaceFileTree::default();
        for (file_index, file) in view.files.iter().enumerate() {
            tree.insert(file.path.to_string_lossy().as_ref(), file_index);
        }
        Self {
            totals: WorkspaceLineTotals::from_files(&view.files),
            tree,
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
    artifact_view_id: Option<String>,
) -> Element {
    let workspace = super::use_workspace_context();
    let scroll = super::panel_scroll::use_panel_scroll(super::panel_scroll::Panel::Files);
    let model = workspace.files.read();
    let artifact_enhancement = artifact_view_id.is_some();

    rsx! {
        ScrollArea {
            class: "diff-files-scroll-panel h-full min-h-0 p-3 compact:p-2.5",
            "data-testid": test_id,
            onmounted: scroll.mount,
            onresize: move |_| scroll.restore.call(()),
            onscroll: scroll.save,
            FilesPanelHeading { file_count: model.file_count, artifact_view_id }
            FilesPanelSummary { totals: model.totals }
            if model.file_count == 0 {
                EmptyNotice { "No changed files" }
            } else {
                {render_file_tree(&model.tree, false, onnavigate, artifact_enhancement)}
            }
        }
    }
}

#[component]
fn FilesPanelHeading(file_count: usize, artifact_view_id: Option<String>) -> Element {
    let file_label = super::file_label(file_count);
    rsx! {
        div { class: "mx-1 mb-2 flex items-baseline justify-between gap-2",
            h2 { class: "font-semibold text-ink", "Files" }
            div { class: "flex items-center gap-1",
                span { class: "text-xs text-ink-3", "{file_count} {file_label}" }
                super::path_filter::PathFilterTrigger { artifact_view_id }
            }
        }
    }
}

#[component]
fn FilesPanelSummary(totals: WorkspaceLineTotals) -> Element {
    rsx! {
        div { class: "mx-0.5 mb-3 flex flex-wrap gap-2",
            DiffLineChangeBadge { kind: DiffLineChangeKind::Added, count: totals.added.value() }
            DiffLineChangeBadge {
                kind: DiffLineChangeKind::Removed,
                count: totals.removed.value(),
            }
        }
    }
}

fn render_file_tree(
    tree: &WorkspaceFileTree,
    nested: bool,
    onnavigate: EventHandler<String>,
    artifact_enhancement: bool,
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
            class: "diff-files-directory-caret group-open:rotate-90 motion-reduce:transition-none",
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
    let anchor_id = file.anchor_id.clone();
    let color = file_status_text_class(file.status);
    let artifact_action = artifact_enhancement.then_some("navigate-file");
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
                "data-gtl-action": artifact_action,
                title: file.path.to_string_lossy().into_owned(),
                onclick: move |_| onnavigate.call(anchor_id.clone()),
                span { class: "diff-files-file-name min-w-0 {color}", "{file_name}" }
                DiffFileStatus { status: file.status }
            }
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
