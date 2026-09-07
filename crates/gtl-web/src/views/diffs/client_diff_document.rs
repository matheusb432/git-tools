#[cfg(feature = "desktop")]
mod copy_context;
mod file;
#[cfg(feature = "desktop")]
mod find;
#[cfg(any(feature = "component-preview", feature = "desktop"))]
pub(super) mod search_bar;

use dioxus::prelude::*;
use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::{ViewerDiffFileId, ViewerViewIdentity};

use self::file::DiffFileCard;
#[cfg(feature = "desktop")]
use crate::app::application_layout::ViewerContext;
#[cfg(feature = "desktop")]
use crate::entities::diffs::use_client_diff_workspace;
#[cfg(feature = "desktop")]
use crate::shared::ui::use_toast;
use crate::{
    entities::diffs::{ClientDiffFileStoreExt, ClientDiffWorkspace, ClientDiffWorkspaceStoreExt},
    shared::ui::EmptyNotice,
};

#[cfg(feature = "desktop")]
#[component]
pub(crate) fn ClientDiffDocument(onopen: Option<EventHandler<ViewerDiffFileId>>) -> Element {
    let diff = super::diff_workspace::use_workspace_context();
    let view = diff.view;
    let workspace = use_client_diff_workspace(view);
    let toast = use_toast();
    let workspace_store = workspace.workspace();
    let rows_loading = use_memo(move || {
        workspace_store.files().iter().any(|file| {
            *file.state().read() == crate::entities::diffs::ClientDiffFileState::Loading
        })
    });
    let retry_allowed = !workspace.row_stream_active();
    let (title, identity) = view.with(|view| (view.title.clone(), view.identity));
    let is_loading = rows_loading();
    use_diff_rows_loading_tab(identity.tab_id, is_loading);

    rsx! {
        section {
            class: "relative col-start-2 row-start-2 h-full min-h-0 min-w-0 overflow-hidden bg-bg",
            aria_label: "Rendered diff",
            oncopy: move |event: ClipboardEvent| {
                let Some(message) = copy_context::copy_selected_diff_lines(&event) else {
                    return;
                };
                toast.ok(message);
            },
            find::DiffFindBar {
                open: diff.find_open,
                identity,
                rows_loading: is_loading,
                workspace: workspace_store,
            }
            DiffDocumentBody {
                title,
                workspace: workspace_store,
                identity,
                folded: diff.files_folded,
                flashing_file: diff.flashing_file,
                is_loading,
                retry_allowed,
                onopen,
                onretry: move |file_id| workspace.retry_file(file_id),
                artifact_tab_id: None,
            }
        }
    }
}

#[cfg(feature = "desktop")]
fn use_diff_rows_loading_tab(tab_id: ViewerTabId, loading: bool) {
    let viewer = use_context::<ViewerContext>();
    let mut reported_tab_id = use_signal(|| None::<ViewerTabId>);
    use_effect(use_reactive(
        (&tab_id, &loading),
        move |(tab_id, loading)| {
            let previous = *reported_tab_id.peek();
            if let Some(previous) = previous.filter(|previous| *previous != tab_id) {
                viewer.set_diff_rows_loading(previous, false);
            }
            viewer.set_diff_rows_loading(tab_id, loading);
            reported_tab_id.set(loading.then_some(tab_id));
        },
    ));
    use_drop(move || {
        if let Some(tab_id) = *reported_tab_id.peek() {
            viewer.set_diff_rows_loading(tab_id, false);
        }
    });
}

#[cfg(feature = "artifact")]
#[component]
pub(crate) fn StaticDiffDocument(
    workspace: ClientDiffWorkspace,
    overlay: Option<Element>,
) -> Element {
    let diff = super::diff_workspace::use_workspace_context();
    let workspace = use_store(move || workspace);
    let title = diff.view.read().title.clone();
    let identity = workspace.identity().cloned();
    rsx! {
        section {
            class: "relative col-start-2 row-start-2 h-full min-h-0 min-w-0 overflow-hidden bg-bg",
            aria_label: "Rendered diff",
            if let Some(overlay) = overlay {
                {overlay}
            }
            DiffDocumentBody {
                title,
                workspace,
                identity,
                folded: diff.files_folded,
                flashing_file: diff.flashing_file,
                is_loading: false,
                retry_allowed: false,
                onopen: None,
                onretry: move |_file_id| {},
                artifact_tab_id: Some(identity.tab_id),
            }
        }
    }
}

#[component]
fn DiffDocumentBody(
    title: String,
    workspace: ReadStore<ClientDiffWorkspace>,
    identity: ViewerViewIdentity,
    folded: ReadSignal<Option<bool>>,
    flashing_file: ReadSignal<Option<String>>,
    is_loading: bool,
    retry_allowed: bool,
    onopen: Option<EventHandler<ViewerDiffFileId>>,
    onretry: EventHandler<ViewerDiffFileId>,
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
            class: "h-full min-h-0 overflow-auto bg-bg pb-[60px] text-ink tablet:pb-12 print:overflow-visible print:p-0",
            role: "region",
            aria_label: "Rendered diff for {title}",
            aria_busy: is_loading.to_string(),
            "data-gtl-diff-document": "",
            "data-view-state": if is_loading { "streaming" } else { "complete" },
            "data-chunks-complete": (!is_loading).to_string(),
            "data-view-identity": view_identity,
            "data-layout": layout.as_str(),
            "data-density": density.as_str(),
            if workspace.files().is_empty() {
                EmptyNotice { "no file changes" }
            }
            for (index, file) in workspace.files().iter().enumerate() {
                {
                    let file_id = file.summary().peek().id.clone();
                    rsx! {
                        DiffFileCard {
                            key: "{file_id.as_str()}",
                            file,
                            layout,
                            density,
                            folded,
                            flashing_file,
                            onopen,
                            onretry: move |()| onretry.call(file_id.clone()),
                            retry_allowed,
                            file_index: index,
                            artifact_tab_id,
                        }
                    }
                }
            }
        }
    }
}
