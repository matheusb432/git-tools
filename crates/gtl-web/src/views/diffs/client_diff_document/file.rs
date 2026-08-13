mod actions;
mod rows;

use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout};

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
    onopen: Option<EventHandler<String>>,
    onretry: EventHandler<()>,
    file_index: usize,
) -> Element {
    let mut open = use_signal(|| file.summary.initially_expanded);
    use_effect(use_reactive((&folded,), move |(folded,)| {
        if let Some(folded) = folded {
            open.set(!folded);
        }
    }));
    let anchor_id = file.summary.anchor_id.clone();
    let path = file.summary.path.clone();

    rsx! {
        details {
            id: anchor_id,
            "data-gtl-diff-file": "",
            "data-path": path,
            // TODO: review stlying
            class: "group/file mb-2.5 rounded-panel border border-line bg-surface [&:not([open])>summary]:rounded-panel [&:not([open])>summary]:border-b-0 print:break-inside-avoid print:[&[hidden]]:block!",
            class: if is_flashing { "outline outline-acc outline-offset-[-1px]" },
            open: open(),
            DiffFileHeader {
                file: file.clone(),
                open,
                copy_context_enabled,
                onopen,
            }
            DiffFileBody {
                file,
                layout,
                density,
                file_index,
                onretry,
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
    onopen: Option<EventHandler<String>>,
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
            DiffFilePath { path: file_summary.path.clone() }
            DiffFileStatusBadge { status: file_summary.status }
            DiffFileActions { file, copy_context_enabled, onopen }
            DiffLineStats { added: file_summary.added, removed: file_summary.removed }
        }
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
fn DiffLineStats(added: u32, removed: u32) -> Element {
    rsx! {
        span { class: "flex-none text-sm",
            DiffLineChangeText { kind: DiffLineChangeKind::Added, count: u64::from(added) }
            " "
            DiffLineChangeText { kind: DiffLineChangeKind::Removed, count: u64::from(removed) }
        }
    }
}
