mod actions;
#[cfg(test)]
mod navigation_tests;
pub(super) mod rows;

use dioxus::prelude::*;
use gtl_models::{diffs::DiffLineCount, paths::RepositoryRelativePath};
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout, ViewerFileStatus};

use self::{actions::DiffFileActions, rows::DiffFileBody};
use crate::{
    entities::diffs::{ClientDiffFile, ClientDiffFileStoreExt},
    views::diffs::{
        DiffFileStatus, DiffLineChangeKind, DiffLineChangeText, file_status_text_class,
    },
};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct DiffFileControls {
    pub(super) open: ReadSignal<bool>,
    pub(super) onchange: Callback<bool>,
    pub(super) onresize: EventHandler<ResizeEvent>,
}

#[component]
pub(super) fn DiffFileCard(
    file: ReadStore<ClientDiffFile>,
    layout: ViewerDiffLayout,
    density: ViewerDiffDensity,
    folded: ReadSignal<Option<bool>>,
    flashing_file: ReadSignal<Option<String>>,
    onopen: Option<EventHandler<ViewerDiffFileId>>,
    onretry: EventHandler<()>,
    retry_allowed: bool,
    file_index: usize,
    controls: Option<DiffFileControls>,
    body: Option<Element>,
) -> Element {
    #[cfg(test)]
    navigation_tests::record_render(file_index);
    let summary = file.summary();
    let initially_expanded = summary.peek().initially_expanded;
    let mut local_open = use_signal(|| initially_expanded);
    let open = controls.map_or_else(|| ReadSignal::from(local_open), |controls| controls.open);
    let set_open = use_callback(move |next: bool| {
        if let Some(controls) = controls {
            controls.onchange.call(next);
        } else if *local_open.peek() != next {
            local_open.set(next);
        }
    });
    use_effect(move || {
        if controls.is_none()
            && let Some(folded) = folded()
        {
            set_open.call(!folded);
        }
    });
    let (anchor_id, comment_leader) = summary.with(|summary| {
        (
            summary.anchor_id.clone(),
            copy_comment_leader(&summary.path),
        )
    });
    let path = summary.with(|summary| summary.path.to_string_lossy().into_owned());
    let navigation_anchor_id = anchor_id.clone();
    use_effect(move || {
        if flashing_file.read().as_deref() == Some(navigation_anchor_id.as_str()) {
            set_open.call(true);
        }
    });
    let copy_popover_id = format!("{anchor_id}-copy-menu");
    let is_flashing = use_memo(move || {
        let summary = summary.read();
        flashing_file
            .read()
            .as_deref()
            .is_some_and(|flashing| flashing == summary.anchor_id)
    });

    rsx! {
        details {
            id: anchor_id,
            "data-gtl-diff-file": "",
            "data-file-index": file_index.to_string(),
            "data-path": path.clone(),
            "data-gtl-comment-leader": comment_leader,
            class: "diff-file-card group/file print:[&[hidden]]:block!",
            class: if is_flashing() { "outline outline-acc outline-offset-[-1px]" },
            open: open(),
            DiffFileHeader {
                summary,
                open,
                onopenchange: set_open,
                onresize: controls.map(|controls| controls.onresize),
                copy_popover_id,
                onopen,
                guide: file_index == 0,
            }
            if let Some(body) = body {
                {body}
            } else {
                DiffFileBody {
                    file,
                    layout,
                    density,
                    file_index,
                    onretry,
                    retry_allowed,
                }
            }
        }
    }
}

#[component]
fn DiffFileHeader(
    summary: ReadSignal<gtl_wire::viewer::ViewerFileSummary>,
    open: ReadSignal<bool>,
    onopenchange: Callback<bool>,
    onresize: Option<EventHandler<ResizeEvent>>,
    copy_popover_id: String,
    onopen: Option<EventHandler<ViewerDiffFileId>>,
    guide: bool,
) -> Element {
    let file_summary = summary.read();
    rsx! {
        summary {
            class: "diff-file-summary",
            "data-tour": super::tour::READING_HEADER.value(),
            onclick: move |event| {
                event.prevent_default();
                onopenchange.call(!*open.peek());
            },
            onresize: move |event| {
                if let Some(onresize) = onresize {
                    onresize.call(event);
                }
            },
            DiffFileCaret {}
            DiffFilePath {
                path: file_summary.path.to_string_lossy().into_owned(),
                status: file_summary.status,
            }
            DiffLineStats { added: file_summary.added, removed: file_summary.removed }
            span {
                class: "h-5 w-px flex-none bg-line mobile:hidden",
                aria_hidden: "true",
            }
            DiffFileActions { summary, copy_popover_id, onopen }
            if guide {
                span {
                    class: "inline-flex",
                    onclick: move |event: MouseEvent| event.stop_propagation(),
                    crate::shared::ui::guided_tour::GuidedTourButton { tour: super::tour::READING }
                }
            }
            DiffFileStatus { status: file_summary.status }
        }
    }
}

pub(super) fn copy_comment_leader(path: &RepositoryRelativePath) -> &'static str {
    match path
        .as_path()
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some(extension) if extension.eq_ignore_ascii_case("sh") => "#",
        _ => "//",
    }
}

#[component]
fn DiffFileCaret() -> Element {
    rsx! {
        span {
            class: "diff-file-caret size-5 text-base leading-none group-open/file:rotate-90 motion-reduce:transition-none",
            aria_hidden: "true",
            "›"
        }
    }
}

#[component]
fn DiffFilePath(path: String, status: ViewerFileStatus) -> Element {
    let color = file_status_text_class(status);
    let (directory, file_name) = path
        .rsplit_once('/')
        .map_or((None, path.as_str()), |(directory, file_name)| {
            (Some(directory), file_name)
        });
    rsx! {
        span { class: "diff-file-path min-w-0",
            if let Some(directory) = directory {
                span { class: "text-ink-3", "{directory}/" }
            }
            span { class: "font-semibold {color}", "{file_name}" }
        }
    }
}

#[component]
fn DiffLineStats(added: DiffLineCount, removed: DiffLineCount) -> Element {
    rsx! {
        span { class: "diff-file-line-stats gap-1.5 text-xs font-medium tabular-nums",
            DiffLineChangeText { kind: DiffLineChangeKind::Added, count: added.value() }
            DiffLineChangeText { kind: DiffLineChangeKind::Removed, count: removed.value() }
        }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::dioxus_core::NoOpMutations;
    use gtl_models::diffs::DiffLineCount;
    use gtl_wire::viewer::{
        ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout, ViewerFileStatus, ViewerFileSummary,
        ViewerRenderOptions, ViewerUnifiedRow, ViewerViewIdentity,
    };

    use super::*;
    use crate::{
        entities::diffs::{
            ClientDiffFileState, ClientDiffRows, ClientDiffRowsStoreExt, ClientDiffWorkspace,
            ClientDiffWorkspaceStoreExt,
        },
        test_support::{
            TestResult, absolute_file_path, repository_relative_path, unified_source_row,
            viewer_tab_id,
        },
    };

    pub(super) fn test_file() -> TestResult<ClientDiffFile> {
        Ok(ClientDiffFile {
            summary: ViewerFileSummary {
                source_id: None,
                id: ViewerDiffFileId::for_index(0),
                path: repository_relative_path("scripts/run.SH")?,
                absolute_path: absolute_file_path("/repo/scripts/run.SH")?,
                anchor_id: "f-scripts-run-sh".to_owned(),
                added: DiffLineCount::new(1),
                removed: DiffLineCount::default(),
                status: ViewerFileStatus::Modified,
                can_open_in_editor: true,
                initially_expanded: false,
                row_count: 1,
            },
            rows: ClientDiffRows {
                unified: vec![vec![
                    ViewerUnifiedRow::Hunk("@@ -1 +1 @@".to_owned()),
                    ViewerUnifiedRow::Added(unified_source_row("echo static", None, Some(1), None)),
                ]],
                split: Vec::new(),
            },
            line_number_digits: 1,
            state: ClientDiffFileState::Complete,
        })
    }

    #[component]
    fn TestDiffFile(file: ClientDiffFile) -> Element {
        let Some(tab_id) = viewer_tab_id(1).ok() else {
            return rsx! {};
        };
        let files = vec![file.clone(), file.clone(), file.clone(), file];
        let workspace = use_store(move || ClientDiffWorkspace {
            identity: ViewerViewIdentity {
                tab_id,
                range_generation: gtl_models::viewer::ViewerRangeGeneration::default(),
                selection_generation: gtl_models::viewer::ViewerSelectionGeneration::default(),
                render_options: ViewerRenderOptions {
                    wrap_lines: false,
                    layout: ViewerDiffLayout::Unified,
                    density: ViewerDiffDensity::Compact,
                },
            },
            files,
        });
        use_context_provider(|| workspace);
        let Some(file) = workspace.files().get(3) else {
            return rsx! {};
        };
        let folded = use_signal(|| None::<bool>);
        let flashing_file = use_signal(|| None::<String>);
        rsx! {
            DiffFileCard {
                file,
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
                folded,
                flashing_file,
                onopen: None,
                onretry: move |()| {},
                retry_allowed: false,
                file_index: 3,
            }
        }
    }

    #[test]
    fn streamed_batches_keep_existing_rows_and_replace_them_on_retry() -> TestResult {
        let mut dom =
            VirtualDom::new_with_props(TestDiffFile, TestDiffFileProps { file: test_file()? });
        dom.rebuild_in_place();
        let workspace = dom
            .runtime()
            .consume_context::<Store<ClientDiffWorkspace>>(ScopeId::APP)
            .unwrap();
        let file = workspace.files().get(3).unwrap();
        file.rows()
            .unified()
            .push(vec![ViewerUnifiedRow::Meta("appended batch".to_owned())]);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("echo static"));
        assert!(html.contains("appended batch"));

        file.rows().set(ClientDiffRows::default());
        dom.render_immediate(&mut NoOpMutations);
        file.rows()
            .unified()
            .push(vec![ViewerUnifiedRow::Meta("retried batch".to_owned())]);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("retried batch"));
        assert!(!html.contains("echo static"));
        assert!(!html.contains("appended batch"));
        Ok(())
    }
}
