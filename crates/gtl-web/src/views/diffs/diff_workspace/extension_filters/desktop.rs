use dioxus::prelude::*;

use super::ExtensionFilterMenu;
use crate::{
    app::application_layout::{ViewerContext, ViewerShellLoad},
    entities::diffs::viewer_server,
    shared::{
        failure_notice::client_error_message,
        i18n::{t, use_language},
        ui::{Button, ButtonSize, ButtonVariant},
    },
    views::diffs::file_filter_changes::FileFilterController,
};

/// Loads the active tab's extension filter and submits edits through the shared controller.
#[component]
pub(in crate::views::diffs::diff_workspace) fn ExtensionFilters(id: String) -> Element {
    let language = use_language();
    let workspace = super::super::use_workspace_context();
    let context = use_context::<ViewerContext>();
    let controller = use_context::<FileFilterController>();
    let identity = use_memo(move || workspace.view.read().identity);
    let refresh = use_memo(move || controller.refresh_epoch());
    let version = use_memo(move || match &*context.shell().read() {
        ViewerShellLoad::Ready(shell) => Some(shell.version),
        _ => None,
    });
    let mut filters = use_resource(move || {
        let tab_id = identity().tab_id;
        let epoch = refresh();
        let _ = version();
        async move {
            (
                tab_id,
                epoch,
                viewer_server::get_file_filters(gtl_wire::viewer::ViewerTabRequest { tab_id })
                    .await,
            )
        }
    });
    let tab_id = identity().tab_id;
    let data = filters.read();
    let fetched = data.as_ref().filter(|(id, _, _)| *id == tab_id);
    let fetched_epoch = fetched.map_or(0, |(_, epoch, _)| *epoch);
    let metadata = fetched.and_then(|(_, _, result)| result.as_ref().ok());
    let view = workspace.view.read();
    let applied = view.extension_filter.as_ref();
    let filter = controller
        .displayed(tab_id, fetched_epoch)
        .or_else(|| metadata.map(|value| value.filter.clone()))
        .or_else(|| applied.map(|value| value.filter.clone()))
        .unwrap_or_default();
    let available = metadata
        .map(|value| value.extensions.clone())
        .unwrap_or_default();
    let hidden_count = applied.map_or(0, |value| value.hidden_paths.len());
    let error = controller
        .error(tab_id)
        .or_else(|| fetched.and_then(|(_, _, result)| result.as_ref().err().cloned()));
    rsx! {
        ExtensionFilterMenu {
            key: "{tab_id}",
            id,
            filter,
            available,
            hidden_count,
            onchange: move |filter| controller.submit(tab_id, filter),
            if let Some(error) = error {
                div { class: "extension-filter-error", role: "alert",
                    p { {client_error_message(&error, language)} }
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| {
                            controller.retry(tab_id);
                            filters.restart();
                        },
                        {t!(language, "action-retry")}
                    }
                }
            }
        }
    }
}
