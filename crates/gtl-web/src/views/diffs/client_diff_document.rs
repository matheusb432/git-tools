mod file;

use dioxus::prelude::*;
use gtl_contracts::viewer::{ViewerActiveView, ViewerDiffDensity, ViewerDiffLayout};

use self::file::DiffFileCard;
use crate::entities::diffs::{ClientDiffFile, ClientDiffSource, use_client_diff_workspace};

#[component]
pub(crate) fn ClientDiffDocument(
    source: ClientDiffSource,
    view: ViewerActiveView,
    folded: Option<bool>,
    copy_context_enabled: bool,
    flashing_file: Option<String>,
    onopen: Option<EventHandler<String>>,
) -> Element {
    let mut reload = use_signal(|| 0_u64);
    let workspace = use_client_diff_workspace(source, view.identity, view.files.clone(), reload());
    let current = workspace();
    let is_loading = current.is_loading();

    rsx! {
        section {
            class: "relative col-start-2 row-start-2 min-h-0 min-w-0 overflow-hidden bg-bg",
            aria_label: "Rendered diff",
            if is_loading {
                DiffStreamingNotice {}
            }
            DiffDocumentBody {
                title: view.title,
                files: current.files,
                layout: view.identity.render_options.layout,
                density: view.identity.render_options.density,
                folded,
                copy_context_enabled,
                flashing_file,
                is_loading,
                onopen,
                onretry: move |()| reload += 1,
            }
        }
    }
}

#[component]
fn DiffStreamingNotice() -> Element {
    rsx! {
        div {
            class: "absolute inset-x-0 top-0 z-10 border-b border-acc-line bg-acc-soft px-3 py-1.5 text-center text-acc",
            role: "status",
            "Loading diff rows"
        }
    }
}

#[component]
fn DiffDocumentBody(
    title: String,
    files: Vec<ClientDiffFile>,
    layout: ViewerDiffLayout,
    density: ViewerDiffDensity,
    folded: Option<bool>,
    copy_context_enabled: bool,
    flashing_file: Option<String>,
    is_loading: bool,
    onopen: Option<EventHandler<String>>,
    onretry: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "h-full min-h-0 overflow-auto bg-bg px-[22px] pb-[60px] text-ink wide-screen:px-7 compact-desktop:px-4 tablet:px-3 tablet:pb-12 mobile:px-1 print:overflow-visible print:p-0",
            role: "region",
            aria_label: "Rendered diff for {title}",
            aria_busy: is_loading.to_string(),
            "data-gtl-diff-document": "",
            "data-view-state": if is_loading { "streaming" } else { "complete" },
            "data-chunks-complete": (!is_loading).to_string(),
            if files.is_empty() {
                DiffEmptyState {}
            }
            for (index, file) in files.into_iter().enumerate() {
                {
                    let is_flashing = flashing_file.as_deref()
                        == Some(file.summary.anchor_id.as_str());
                    rsx! {
                        DiffFileCard {
                            key: "{file.summary.id.as_str()}",
                            file,
                            layout,
                            density,
                            folded,
                            copy_context_enabled,
                            is_flashing,
                            onopen,
                            onretry,
                            file_index: index,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DiffEmptyState() -> Element {
    rsx! {
        div { class: "rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic",
            "no file changes"
        }
    }
}
