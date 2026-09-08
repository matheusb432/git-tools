mod actions;
#[cfg(test)]
mod navigation_tests;
mod rows;

use dioxus::prelude::*;
use gtl_models::{diffs::DiffLineCount, paths::RepositoryRelativePath, viewer::ViewerTabId};
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout, ViewerFileStatus};

use self::{actions::DiffFileActions, rows::DiffFileBody};
use crate::{
    entities::diffs::{ClientDiffFile, ClientDiffFileStoreExt},
    views::diffs::{DiffFileStatusBadge, DiffLineChangeKind, DiffLineChangeText},
};

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
    artifact_tab_id: Option<ViewerTabId>,
) -> Element {
    #[cfg(test)]
    navigation_tests::record_render(file_index);
    let summary = file.summary();
    let initially_expanded = summary.peek().initially_expanded;
    let mut open = use_signal(|| initially_expanded);
    use_effect(move || {
        if let Some(folded) = folded() {
            open.set(!folded);
        }
    });
    let (file_id, original_anchor_id, path, absolute_path, comment_leader) =
        summary.with(|summary| {
            (
                summary.id.clone(),
                summary.anchor_id.clone(),
                summary.path.to_string_lossy().into_owned(),
                summary
                    .absolute_path
                    .as_path()
                    .to_string_lossy()
                    .into_owned(),
                copy_comment_leader(&summary.path),
            )
        });
    let artifact_file_id = artifact_tab_id.map(|tab_id| static_artifact_file_id(tab_id, &file_id));
    let anchor_id = artifact_file_id.clone().unwrap_or(original_anchor_id);
    let navigation_anchor_id = anchor_id.clone();
    use_effect(move || {
        if flashing_file.read().as_deref() == Some(navigation_anchor_id.as_str()) {
            open.set(true);
        }
    });
    let copy_popover_id = format!("{anchor_id}-copy-menu");
    let artifact_path = artifact_file_id.as_ref().map(|_| path.clone());
    let artifact_absolute_path = artifact_file_id.as_ref().map(|_| absolute_path);
    let artifact_enhancement = artifact_file_id.is_some();
    let artifact_initial_open =
        artifact_enhancement.then_some(if initially_expanded { "true" } else { "false" });
    let is_flashing = use_memo(move || {
        let summary = summary.read();
        flashing_file
            .read()
            .as_deref()
            .is_some_and(|flashing| match artifact_tab_id {
                Some(tab_id) => flashing == static_artifact_file_id(tab_id, &summary.id),
                None => flashing == summary.anchor_id,
            })
    });

    rsx! {
        details {
            id: anchor_id,
            "data-gtl-diff-file": "",
            "data-path": path.clone(),
            "data-gtl-file": artifact_file_id.clone(),
            "data-gtl-path": artifact_path,
            "data-gtl-absolute-path": artifact_absolute_path,
            "data-gtl-comment-leader": comment_leader,
            "data-gtl-initial-open": artifact_initial_open,
            class: "group/file border-b border-line-2 bg-bg print:break-inside-avoid print:[&[hidden]]:block!",
            class: if is_flashing() { "outline outline-acc outline-offset-[-1px]" },
            open: open(),
            DiffFileHeader {
                summary,
                open,
                copy_popover_id,
                onopen,
                artifact_enhancement,
            }
            DiffFileBody {
                file,
                layout,
                density,
                file_index,
                onretry,
                retry_allowed,
                artifact_file_id: artifact_file_id.clone(),
            }
        }
    }
}

#[component]
fn DiffFileHeader(
    summary: ReadSignal<gtl_wire::viewer::ViewerFileSummary>,
    mut open: Signal<bool>,
    copy_popover_id: String,
    onopen: Option<EventHandler<ViewerDiffFileId>>,
    artifact_enhancement: bool,
) -> Element {
    let file_summary = summary.read();
    let status_rail_classes = diff_file_status_rail_classes(file_summary.status);
    rsx! {
        summary {
            class: "relative sticky top-0 z-10 flex min-h-10 cursor-pointer list-none items-center gap-2 border-b-0 border-line-2 bg-surface py-1 pr-3 pl-5 hover:bg-surface-2 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc group-open/file:border-b [&::-webkit-details-marker]:hidden mobile:min-h-11 mobile:gap-1.5 mobile:py-1.5 mobile:pr-2 mobile:pl-4 print:static print:bg-[#f2f2f2]",
            onclick: move |event| {
                event.prevent_default();
                open.toggle();
            },
            span {
                class: "pointer-events-none absolute inset-y-0 left-0 w-0.5 {status_rail_classes} print:hidden",
                aria_hidden: "true",
            }
            DiffFileCaret {}
            DiffFileStatusBadge { status: file_summary.status }
            DiffFilePath { path: file_summary.path.to_string_lossy().into_owned() }
            DiffLineStats { added: file_summary.added, removed: file_summary.removed }
            span {
                class: "h-5 w-px flex-none bg-line mobile:hidden",
                aria_hidden: "true",
            }
            DiffFileActions {
                summary,
                copy_popover_id,
                onopen,
                artifact_enhancement,
            }
        }
    }
}

const fn diff_file_status_rail_classes(status: ViewerFileStatus) -> &'static str {
    match status {
        ViewerFileStatus::Added => "bg-add",
        ViewerFileStatus::Deleted => "bg-del",
        ViewerFileStatus::Renamed | ViewerFileStatus::Modified => "bg-acc",
    }
}

fn static_artifact_file_id(tab_id: ViewerTabId, file_id: &ViewerDiffFileId) -> String {
    format!("artifact-view-{tab_id}-{}", file_id.as_str())
}

fn copy_comment_leader(path: &RepositoryRelativePath) -> &'static str {
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
            class: "flex size-5 flex-none items-center justify-center text-base leading-none text-ink-3 transition-transform group-open/file:rotate-90 motion-reduce:transition-none",
            aria_hidden: "true",
            "›"
        }
    }
}

#[component]
fn DiffFilePath(path: String) -> Element {
    let (directory, file_name) = path
        .rsplit_once('/')
        .map_or((None, path.as_str()), |(directory, file_name)| {
            (Some(directory), file_name)
        });
    rsx! {
        span { class: "min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap",
            if let Some(directory) = directory {
                span { class: "text-ink-3", "{directory}/" }
            }
            span { class: "font-semibold text-ink", "{file_name}" }
        }
    }
}

#[component]
fn DiffLineStats(added: DiffLineCount, removed: DiffLineCount) -> Element {
    rsx! {
        span { class: "flex flex-none items-center gap-1.5 text-xs font-medium tabular-nums",
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
                id: ViewerDiffFileId::for_index(0),
                path: repository_relative_path("scripts/run.SH")?,
                absolute_path: absolute_file_path("/repo/scripts/run.SH")?,
                anchor_id: "f-scripts-run-sh".to_owned(),
                added: DiffLineCount::new(1),
                removed: DiffLineCount::default(),
                status: ViewerFileStatus::Modified,
                can_open_in_editor: true,
                initially_expanded: false,
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

    fn render_file(
        file: ClientDiffFile,
        artifact_tab_id: Option<gtl_models::viewer::ViewerTabId>,
    ) -> String {
        let mut file_card = VirtualDom::new_with_props(
            TestDiffFile,
            TestDiffFileProps {
                file,
                artifact_tab_id,
            },
        );
        file_card.rebuild_in_place();
        dioxus_ssr::render(&file_card)
    }

    #[component]
    fn TestDiffFile(
        file: ClientDiffFile,
        artifact_tab_id: Option<gtl_models::viewer::ViewerTabId>,
    ) -> Element {
        let Some(tab_id) = artifact_tab_id.or_else(|| viewer_tab_id(1).ok()) else {
            return rsx! {};
        };
        let files = vec![file.clone(), file.clone(), file.clone(), file];
        let workspace = use_store(move || ClientDiffWorkspace {
            identity: ViewerViewIdentity {
                tab_id,
                range_generation: gtl_models::viewer::ViewerRangeGeneration::default(),
                selection_generation: gtl_models::viewer::ViewerSelectionGeneration::default(),
                render_options: ViewerRenderOptions {
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
                artifact_tab_id,
            }
        }
    }

    #[test]
    fn streamed_batches_keep_existing_rows_and_replace_them_on_retry() -> TestResult {
        let mut dom = VirtualDom::new_with_props(
            TestDiffFile,
            TestDiffFileProps {
                file: test_file()?,
                artifact_tab_id: None,
            },
        );
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

    #[test]
    fn artifact_file_markup_is_qualified_and_has_compact_copy_actions() -> TestResult {
        let artifact = render_file(test_file()?, Some(viewer_tab_id(7)?));

        assert!(artifact.contains(r#"id="artifact-view-7-file-0""#));
        assert!(artifact.contains(r#"id="artifact-view-7-file-0-rows""#));
        assert!(artifact.contains(r#"data-gtl-file="artifact-view-7-file-0""#));
        assert!(artifact.contains(r#"data-gtl-path="scripts/run.SH""#));
        assert!(artifact.contains(r#"data-gtl-absolute-path="/repo/scripts/run.SH""#));
        assert!(artifact.contains("data-gtl-comment-leader=\"#\""));
        assert!(artifact.contains(r#"data-gtl-initial-open="false""#));
        assert!(artifact.contains("group/file border-b border-line-2 bg-bg"));
        assert!(artifact.contains("relative sticky top-0 z-10"));
        assert!(artifact.contains("absolute inset-y-0 left-0 w-0.5 bg-acc"));
        assert!(!artifact.contains("mb-5 rounded-panel"));
        assert_artifact_copy_menu(&artifact);

        let other_artifact = render_file(test_file()?, Some(viewer_tab_id(8)?));
        assert!(other_artifact.contains(r#"id="artifact-view-8-file-0""#));
        assert!(other_artifact.contains(r#"id="artifact-view-8-file-0-rows""#));
        assert!(other_artifact.contains(r#"id="artifact-view-8-file-0-copy-menu""#));
        assert!(!other_artifact.contains(r#"id="artifact-view-7-file-0""#));

        let desktop = render_file(test_file()?, None);
        assert!(desktop.contains(r#"id="f-scripts-run-sh""#));
        assert!(desktop.contains(r#"id="f-scripts-run-sh-copy-menu""#));
        assert!(desktop.contains(r#"id="viewer-diff-3""#));
        assert!(desktop.contains("data-gtl-comment-leader=\"#\""));
        assert!(desktop.contains(r#"data-gtl-copy-line="""#));
        assert!(!desktop.contains("data-gtl-file="));
        assert!(!desktop.contains("data-gtl-copy="));
        Ok(())
    }

    #[test]
    fn file_status_rail_uses_the_file_status_tone() {
        assert_eq!(
            diff_file_status_rail_classes(ViewerFileStatus::Added),
            "bg-add"
        );
        assert_eq!(
            diff_file_status_rail_classes(ViewerFileStatus::Deleted),
            "bg-del"
        );
        assert_eq!(
            diff_file_status_rail_classes(ViewerFileStatus::Renamed),
            "bg-acc"
        );
        assert_eq!(
            diff_file_status_rail_classes(ViewerFileStatus::Modified),
            "bg-acc"
        );
    }

    fn assert_artifact_copy_menu(artifact: &str) {
        assert!(artifact.contains(r#"id="artifact-view-7-file-0-copy-menu""#));
        assert!(artifact.contains(r#"popovertarget="artifact-view-7-file-0-copy-menu""#));
        assert!(artifact.contains(r#"popover="auto""#));
        assert!(artifact.contains("[position-area:bottom_span-left]"));
        assert!(artifact.contains("[position-try-fallbacks:flip-block]"));
        assert!(artifact.contains(r#"data-gtl-copy="path""#));
        assert!(artifact.contains(r#"data-gtl-copy="absolute""#));
        assert!(!artifact.contains(r#"data-gtl-copy="code""#));
        assert_eq!(artifact.matches("data-gtl-copy-feedback=").count(), 2);
        assert!(artifact.contains("Relative path"));
        assert!(artifact.contains("Absolute path"));
        assert!(!artifact.contains("<small"));
        assert!(!artifact.contains(r#"title="scripts/run.SH""#));
        assert!(!artifact.contains(r#"title="/repo/scripts/run.SH""#));
    }
}
