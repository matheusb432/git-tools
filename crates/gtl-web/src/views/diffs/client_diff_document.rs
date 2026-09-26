#[cfg(feature = "artifact")]
use crate::shared::ui::ScrollArea;
use crate::shared::{
    diff_view_title::diff_view_title_text,
    i18n::{t, use_language},
};
#[cfg(feature = "desktop")]
mod copy_context;
mod file;
#[cfg(feature = "desktop")]
mod find;
mod scroll_area;
#[cfg(any(feature = "component-preview", feature = "desktop"))]
pub(super) mod search_bar;
#[cfg(feature = "desktop")]
pub(super) mod viewport;

#[cfg(feature = "desktop")]
use dioxus::core::Task;
use dioxus::prelude::*;
use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::{ViewerDiffFileId, ViewerViewIdentity};

#[cfg(feature = "artifact")]
use self::file::DiffFileCard;
#[cfg(feature = "desktop")]
use crate::app::application_layout::ViewerContext;
#[cfg(feature = "desktop")]
use crate::entities::diffs::use_client_diff_workspace;
use crate::entities::diffs::{
    ClientDiffFileStoreExt, ClientDiffWorkspace, ClientDiffWorkspaceStoreExt,
};
#[cfg(feature = "artifact")]
use crate::shared::ui::EmptyNotice;

#[cfg(feature = "desktop")]
#[derive(Clone, Copy, PartialEq, Eq)]
struct DiffSearchTarget {
    identity: ViewerViewIdentity,
    file: usize,
    row: usize,
}

#[cfg(feature = "desktop")]
#[component]
pub(crate) fn ClientDiffDocument(onopen: Option<EventHandler<ViewerDiffFileId>>) -> Element {
    let diff = super::diff_workspace::use_workspace_context();
    let view = diff.view;
    let workspace = use_client_diff_workspace(view);
    let identity = view.read().identity;
    let row_source = view.read().row_source;
    let workspace_store = workspace.workspace();
    let stream_active = workspace.row_stream_active();
    let is_loading = stream_active;
    let retry_allowed = !stream_active && workspace_store.is_some_and(|store| {
        store.files().iter().any(|file| {
            matches!(&*file.state().read(), crate::entities::diffs::ClientDiffFileState::Error(error) if error.retryable())
        })
    });
    use_diff_rows_loading_tab(identity.tab_id, is_loading);

    rsx! {
        if row_source != gtl_wire::viewer::ViewerRowSourceState::Ready {
            DiffSourcePreparation { state: row_source, tab_id: identity.tab_id }
        } else if let Some(workspace_store) = workspace_store {
            LoadedDiffDocument {
                workspace: workspace_store,
                controller: workspace,
                is_loading,
                retry_allowed,
                onopen,
                onretry: move |file_id| workspace.retry_file(&file_id),
            }
        }
    }
}

#[cfg(feature = "desktop")]
#[component]
fn DiffSourcePreparation(
    state: gtl_wire::viewer::ViewerRowSourceState,
    tab_id: ViewerTabId,
) -> Element {
    use crate::shared::ui::{Button, ButtonVariant, PageNotice};
    let language = use_language();
    let viewer = use_context::<ViewerContext>();
    match state {
        gtl_wire::viewer::ViewerRowSourceState::Pending => rsx! {
            div {
                class: "flex h-full items-center justify-center text-sm text-ink-3",
                role: "status",
                {t!(language, "diff-preparing")}
            }
        },
        gtl_wire::viewer::ViewerRowSourceState::Failed => rsx! {
            PageNotice {
                class: "h-full px-5",
                role: "alert",
                title: t!(language, "diff-source-unavailable"),
                message: t!(language, "diff-source-unavailable-message"),
                Button {
                    variant: ButtonVariant::Outline,
                    onclick: move |_| viewer.refresh_tab(tab_id),
                    {t!(language, "diff-refresh")}
                }
            }
        },
        gtl_wire::viewer::ViewerRowSourceState::Ready => rsx! {},
    }
}

#[cfg(feature = "desktop")]
#[component]
fn LoadedDiffDocument(
    workspace: Store<ClientDiffWorkspace>,
    controller: crate::entities::diffs::ClientDiffWorkspaceController,
    is_loading: bool,
    retry_allowed: bool,
    onopen: Option<EventHandler<ViewerDiffFileId>>,
    onretry: EventHandler<ViewerDiffFileId>,
) -> Element {
    let workspace = use_memo(use_reactive((&workspace,), |(workspace,)| {
        ReadStore::from(workspace)
    }))();
    let diff = super::diff_workspace::use_workspace_context();
    let view = diff.view;
    let search_target = use_signal(|| None::<DiffSearchTarget>);
    let language = use_language();
    let (title, identity, content_id) = view.with(|view| {
        (
            diff_view_title_text(&view.title, language),
            view.identity,
            view.content_id,
        )
    });
    copy_context::use_diff_copy(identity, workspace);

    rsx! {
        section {
            class: "relative h-full min-h-0 min-w-0 overflow-hidden bg-bg",
            aria_label: t!(use_language(), "diff-rendered"),
            // Dioxus reconciles keys in lists; each tab owns these hook lifetimes.
            for identity in [identity] {
                find::DiffFindBar {
                    key: "{identity.tab_id}:{content_id:?}:{identity.render_options.wrap_lines}",
                    open: diff.find_open,
                    identity,
                    workspace,
                    target: search_target,
                }
                viewport::DiffViewport {
                    title: title.clone(),
                    workspace,
                    identity,
                    content_id,
                    controller,
                    search_target,
                    folded: diff.files_folded,
                    fold_command: diff.fold_command,
                    flashing_file: diff.flashing_file,
                    is_loading,
                    retry_allowed,
                    onopen,
                    onretry,
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
fn use_diff_rows_loading_tab(tab_id: ViewerTabId, loading: bool) {
    let viewer = use_context::<ViewerContext>();
    let mut reported_tab_id = use_signal(|| None::<ViewerTabId>);
    let mut delay = use_signal(|| None::<Task>);
    use_effect(use_reactive(
        (&tab_id, &loading),
        move |(tab_id, loading)| {
            if let Some(task) = delay.take() {
                task.cancel();
            }
            if let Some(previous) = reported_tab_id.take() {
                viewer.set_diff_rows_loading(previous, false);
            }
            if !loading {
                return;
            }
            delay.set(Some(spawn(async move {
                dioxus_sdk_time::sleep(std::time::Duration::from_millis(150)).await;
                viewer.set_diff_rows_loading(tab_id, true);
                reported_tab_id.set(Some(tab_id));
            })));
        },
    ));
    use_drop(move || {
        if let Some(task) = delay.take() {
            task.cancel();
        }
        if let Some(tab_id) = reported_tab_id.take() {
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
    let title = diff_view_title_text(&diff.view.read().title, use_language());
    let identity = workspace.identity().cloned();
    rsx! {
        section {
            class: "relative h-full min-h-0 min-w-0 overflow-hidden bg-bg",
            aria_label: t!(use_language(), "diff-rendered"),
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

#[cfg(feature = "artifact")]
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
        ScrollArea {
            class: "diff-document-scroll h-full min-h-0 print:overflow-visible",
            role: "region",
            aria_label: t!(use_language(), "diff-rendered-for", title = title.as_str()),
            aria_busy: is_loading.to_string(),
            "data-gtl-diff-document": "",
            "data-wrap-lines": identity.render_options.wrap_lines.to_string(),
            "data-view-state": if is_loading { "streaming" } else { "complete" },
            "data-chunks-complete": (!is_loading).to_string(),
            "data-view-identity": view_identity,
            "data-layout": layout.as_str(),
            "data-density": density.as_str(),
            DiffDocumentFiles {
                workspace,
                layout,
                density,
                folded,
                flashing_file,
                retry_allowed,
                onopen,
                onretry,
                artifact_tab_id,
            }
            div { class: "h-15 tablet:h-12 print:hidden", aria_hidden: "true" }
        }
    }
}

#[cfg(feature = "artifact")]
#[component]
fn DiffDocumentFiles(
    workspace: ReadStore<ClientDiffWorkspace>,
    layout: gtl_wire::viewer::ViewerDiffLayout,
    density: gtl_wire::viewer::ViewerDiffDensity,
    folded: ReadSignal<Option<bool>>,
    flashing_file: ReadSignal<Option<String>>,
    retry_allowed: bool,
    onopen: Option<EventHandler<ViewerDiffFileId>>,
    onretry: EventHandler<ViewerDiffFileId>,
    artifact_tab_id: Option<ViewerTabId>,
) -> Element {
    rsx! {
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
