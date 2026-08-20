mod actions;
mod rows;

use dioxus::prelude::*;
use gtl_models::{diffs::DiffLineCount, paths::RepositoryRelativePath, viewer::ViewerTabId};
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout};

use self::{actions::DiffFileActions, rows::DiffFileBody};
use crate::{
    entities::diffs::ClientDiffFile,
    views::diffs::{DiffFileStatusBadge, DiffLineChangeKind, DiffLineChangeText},
};

#[component]
pub(super) fn DiffFileCard(
    file: ClientDiffFile,
    layout: ViewerDiffLayout,
    density: ViewerDiffDensity,
    folded: Option<bool>,
    copy_context_enabled: bool,
    is_flashing: bool,
    onopen: Option<EventHandler<RepositoryRelativePath>>,
    onretry: EventHandler<()>,
    file_index: usize,
    artifact_tab_id: Option<ViewerTabId>,
) -> Element {
    let mut open = use_signal(|| file.summary.initially_expanded);
    use_effect(use_reactive((&folded,), move |(folded,)| {
        if let Some(folded) = folded {
            open.set(!folded);
        }
    }));
    let artifact_file_id =
        artifact_tab_id.map(|tab_id| static_artifact_file_id(tab_id, &file.summary.id));
    let anchor_id = artifact_file_id
        .clone()
        .unwrap_or_else(|| file.summary.anchor_id.clone());
    let path = file.summary.path.to_string_lossy().into_owned();
    let absolute_path = file
        .summary
        .absolute_path
        .as_path()
        .to_string_lossy()
        .into_owned();
    let artifact_path = artifact_file_id.as_ref().map(|_| path.clone());
    let artifact_absolute_path = artifact_file_id.as_ref().map(|_| absolute_path);
    let artifact_comment_leader = artifact_file_id
        .as_ref()
        .map(|_| copy_comment_leader(&file.summary.path));
    let artifact_initial_open = artifact_file_id.as_ref().map(|_| {
        if file.summary.initially_expanded {
            "true"
        } else {
            "false"
        }
    });
    let artifact_enhancement = artifact_file_id.is_some();

    rsx! {
        details {
            id: anchor_id,
            "data-gtl-diff-file": "",
            "data-path": path.clone(),
            "data-gtl-file": artifact_file_id.clone(),
            "data-gtl-path": artifact_path,
            "data-gtl-absolute-path": artifact_absolute_path,
            "data-gtl-comment-leader": artifact_comment_leader,
            "data-gtl-initial-open": artifact_initial_open,
            // TODO: review stlying
            class: "group/file mb-2.5 rounded-panel border border-line bg-surface [&:not([open])>summary]:rounded-panel [&:not([open])>summary]:border-b-0 print:break-inside-avoid print:[&[hidden]]:block!",
            class: if is_flashing { "outline outline-acc outline-offset-[-1px]" },
            open: open(),
            DiffFileHeader {
                file: file.clone(),
                open,
                copy_context_enabled,
                onopen,
                artifact_enhancement,
            }
            DiffFileBody {
                file,
                layout,
                density,
                file_index,
                onretry,
                artifact_file_id: artifact_file_id.clone(),
            }
        }
    }
}

#[component]
fn DiffFileHeader(
    // TODO: refactor - this must **not** need the entire diff rows!
    // this must be shared state too, not drilled props.
    file: ClientDiffFile,
    mut open: Signal<bool>,
    copy_context_enabled: bool,
    onopen: Option<EventHandler<RepositoryRelativePath>>,
    artifact_enhancement: bool,
) -> Element {
    let background_classes = file_header_background(file.summary.status);
    let file_summary = file.summary.clone();
    rsx! {
        summary {
            class: "sticky top-0 z-2 flex cursor-pointer list-none items-center gap-2 rounded-t-panel border-b border-line px-2.5 py-2 hover:bg-line focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc [&::-webkit-details-marker]:hidden mobile:flex-wrap mobile:gap-x-1.5 mobile:px-2 mobile:py-1.5 print:static print:bg-[#f2f2f2]",
            class: "{background_classes}",
            onclick: move |event| {
                event.prevent_default();
                open.toggle();
            },
            DiffFileCaret {}
            DiffFilePath { path: file_summary.path.to_string_lossy().into_owned() }
            DiffFileStatusBadge { status: file_summary.status }
            DiffFileActions {
                file,
                copy_context_enabled,
                onopen,
                artifact_enhancement,
            }
            DiffLineStats { added: file_summary.added, removed: file_summary.removed }
        }
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

const fn file_header_background(status: gtl_wire::viewer::ViewerFileStatus) -> &'static str {
    use gtl_wire::viewer::ViewerFileStatus;

    match status {
        ViewerFileStatus::Added => "bg-[color-mix(in_srgb,var(--add-bg)_34%,var(--surface-2))]",
        ViewerFileStatus::Deleted => "bg-[color-mix(in_srgb,var(--del-bg)_34%,var(--surface-2))]",
        ViewerFileStatus::Renamed | ViewerFileStatus::Modified => "bg-surface-2",
    }
}

#[component]
fn DiffFileCaret() -> Element {
    rsx! {
        span {
            class: "size-0 flex-none border-y-4 border-y-transparent border-l-5 border-l-ink-3 group-open/file:rotate-90",
            aria_hidden: "true",
        }
    }
}

#[component]
fn DiffFilePath(path: String) -> Element {
    rsx! {
        span { class: "min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-ink",
            "{path}"
        }
    }
}

#[component]
fn DiffLineStats(added: DiffLineCount, removed: DiffLineCount) -> Element {
    rsx! {
        span { class: "flex-none text-sm",
            DiffLineChangeText { kind: DiffLineChangeKind::Added, count: added.value() }
            " "
            DiffLineChangeText { kind: DiffLineChangeKind::Removed, count: removed.value() }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gtl_models::diffs::DiffLineCount;
    use gtl_parser::DiffParser;
    use gtl_wire::viewer::{
        ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout, ViewerFileStatus, ViewerFileSummary,
    };

    use super::*;
    use crate::{
        entities::diffs::{ClientDiffFileState, ClientDiffRows},
        test_support::{TestResult, absolute_file_path, repository_relative_path, viewer_tab_id},
    };

    fn test_file() -> TestResult<ClientDiffFile> {
        let parsed =
            DiffParser::new().parse(&["@@ -1 +1 @@".to_owned(), "+echo static".to_owned()]);
        let line_number_digits = parsed.line_number_digits();

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
            rows: ClientDiffRows::Unified(vec![Arc::new(parsed.into_rows())]),
            line_number_digits,
            state: ClientDiffFileState::Complete,
        })
    }

    fn render_file(
        file: ClientDiffFile,
        artifact_tab_id: Option<gtl_models::viewer::ViewerTabId>,
    ) -> String {
        let event_handler_owner = VirtualDom::new(VNode::empty);
        let props = event_handler_owner.in_scope(ScopeId::ROOT, || DiffFileCardProps {
            file,
            layout: ViewerDiffLayout::Unified,
            density: ViewerDiffDensity::Compact,
            folded: None,
            copy_context_enabled: true,
            is_flashing: false,
            onopen: None,
            onretry: EventHandler::new(|()| {}),
            file_index: 3,
            artifact_tab_id,
        });
        let mut file_card = VirtualDom::new_with_props(DiffFileCard, props);
        file_card.rebuild_in_place();
        dioxus_ssr::render(&file_card)
    }

    #[test]
    fn artifact_file_markup_is_qualified_and_describes_copy_actions() -> TestResult {
        let artifact = render_file(test_file()?, Some(viewer_tab_id(7)?));

        assert!(artifact.contains(r#"id="artifact-view-7-file-0""#));
        assert!(artifact.contains(r#"id="artifact-view-7-file-0-rows""#));
        assert!(artifact.contains(r#"data-gtl-file="artifact-view-7-file-0""#));
        assert!(artifact.contains(r#"data-gtl-path="scripts/run.SH""#));
        assert!(artifact.contains(r#"data-gtl-absolute-path="/repo/scripts/run.SH""#));
        assert!(artifact.contains("data-gtl-comment-leader=\"#\""));
        assert!(artifact.contains(r#"data-gtl-initial-open="false""#));
        assert!(artifact.contains(r#"data-gtl-copy="path""#));
        assert!(artifact.contains(r#"data-gtl-copy="absolute""#));
        assert!(artifact.contains(r#"data-gtl-copy="code""#));
        assert_eq!(artifact.matches("data-gtl-idle-classes=").count(), 3);
        assert_eq!(artifact.matches("data-gtl-success-classes=").count(), 3);
        assert_eq!(artifact.matches("data-gtl-failure-classes=").count(), 3);

        let other_artifact = render_file(test_file()?, Some(viewer_tab_id(8)?));
        assert!(other_artifact.contains(r#"id="artifact-view-8-file-0""#));
        assert!(other_artifact.contains(r#"id="artifact-view-8-file-0-rows""#));
        assert!(!other_artifact.contains(r#"id="artifact-view-7-file-0""#));

        let desktop = render_file(test_file()?, None);
        assert!(desktop.contains(r#"id="f-scripts-run-sh""#));
        assert!(desktop.contains(r#"id="viewer-diff-3""#));
        assert!(!desktop.contains("data-gtl-file="));
        assert!(!desktop.contains("data-gtl-copy="));
        Ok(())
    }
}
