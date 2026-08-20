mod file;

use dioxus::prelude::*;
use gtl_models::{paths::RepositoryRelativePath, viewer::ViewerTabId};
use gtl_wire::viewer::{ViewerActiveView, ViewerViewIdentity};

use self::file::DiffFileCard;
#[cfg(feature = "artifact")]
use crate::entities::diffs::ClientDiffWorkspace;
#[cfg(feature = "desktop")]
use crate::entities::diffs::{ClientDiffSource, use_client_diff_workspace};
use crate::{entities::diffs::ClientDiffFile, shared::ui::EmptyNotice};

#[cfg(feature = "desktop")]
#[component]
pub(crate) fn ClientDiffDocument(
    source: ClientDiffSource,
    view: ViewerActiveView,
    folded: Option<bool>,
    copy_context_enabled: bool,
    flashing_file: Option<String>,
    onopen: Option<EventHandler<RepositoryRelativePath>>,
) -> Element {
    let mut reload = use_signal(|| 0_u64);
    let workspace = use_client_diff_workspace(source, view.identity, &view.files, reload());
    let current = workspace();
    let is_loading = current.is_loading();

    rsx! {
        section {
            class: "relative col-start-2 row-start-2 h-full min-h-0 min-w-0 overflow-hidden bg-bg",
            aria_label: "Rendered diff",
            if is_loading {
                DiffStreamingNotice {}
            }
            DiffDocumentBody {
                title: view.title,
                files: current.files,
                identity: view.identity,
                folded,
                copy_context_enabled,
                flashing_file,
                is_loading,
                onopen,
                onretry: move |()| reload += 1,
                artifact_tab_id: None,
            }
        }
    }
}

#[cfg(feature = "artifact")]
#[component]
pub(crate) fn StaticDiffDocument(
    view: ViewerActiveView,
    workspace: ClientDiffWorkspace,
) -> Element {
    rsx! {
        section {
            class: "relative col-start-2 row-start-2 h-full min-h-0 min-w-0 overflow-hidden bg-bg",
            aria_label: "Rendered diff",
            DiffDocumentBody {
                title: view.title,
                files: workspace.files,
                identity: workspace.identity,
                folded: None,
                copy_context_enabled: true,
                flashing_file: None,
                is_loading: false,
                onopen: None,
                onretry: move |()| {},
                artifact_tab_id: Some(workspace.identity.tab_id),
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
    identity: ViewerViewIdentity,
    folded: Option<bool>,
    copy_context_enabled: bool,
    flashing_file: Option<String>,
    is_loading: bool,
    onopen: Option<EventHandler<RepositoryRelativePath>>,
    onretry: EventHandler<()>,
    artifact_tab_id: Option<ViewerTabId>,
) -> Element {
    let layout = identity.render_options.layout;
    let density = identity.render_options.density;
    let view_identity = format!(
        "{}:{}:{}:{}:{}",
        identity.tab_id,
        identity.range_generation.value(),
        identity.selection_generation.value(),
        layout.as_str(),
        density.as_str(),
    );
    rsx! {
        div {
            class: "h-full min-h-0 overflow-auto bg-bg px-[22px] pb-[60px] text-ink wide-screen:px-7 compact-desktop:px-4 tablet:px-3 tablet:pb-12 mobile:px-1 print:overflow-visible print:p-0",
            role: "region",
            aria_label: "Rendered diff for {title}",
            aria_busy: is_loading.to_string(),
            "data-gtl-diff-document": "",
            "data-view-state": if is_loading { "streaming" } else { "complete" },
            "data-chunks-complete": (!is_loading).to_string(),
            "data-view-identity": view_identity,
            "data-layout": layout.as_str(),
            "data-density": density.as_str(),
            if files.is_empty() {
                EmptyNotice { "no file changes" }
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
                            artifact_tab_id,
                        }
                    }
                }
            }
        }
    }
}
