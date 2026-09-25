use dioxus::prelude::*;
use gtl_models::diffs::ExcludedExtensions;
use gtl_wire::viewer::FieldUpdate;

use super::ExtensionFilter;
use crate::{
    app::application_layout::{ViewerContext, ViewerShellLoad},
    entities::diffs::viewer_server,
    shared::{
        failure_notice::client_error_message,
        i18n::{t, use_language},
        ui::{Button, ButtonSize, ButtonVariant, ExtensionExclusionsAction},
    },
    views::diffs::file_filter_changes::FileFilterController,
};

#[component]
pub(in crate::views::diffs::diff_workspace) fn ExtensionFilters() -> Element {
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
    let metadata = fetched.and_then(|(_, _, result)| result.as_ref().ok());
    let excluded = controller
        .displayed(tab_id, fetched.map_or(0, |(_, epoch, _)| *epoch))
        .or_else(|| metadata.map(|filters| filters.excluded.clone()))
        .unwrap_or_else(|| {
            workspace
                .view
                .read()
                .exclusions
                .as_ref()
                .map_or_else(ExcludedExtensions::default, |value| {
                    value.extensions.clone()
                })
        });
    let fetched_epoch = fetched.map_or(0, |(_, epoch, _)| *epoch);
    let fallback = excluded.clone();
    let defaults = metadata.map_or_else(|| excluded.clone(), |value| value.defaults.clone());
    let available = metadata
        .map(|value| value.extensions.clone())
        .unwrap_or_default();
    let error = controller
        .error(tab_id)
        .or_else(|| fetched.and_then(|(_, _, result)| result.as_ref().err().cloned()));
    rsx! {
        ExtensionFilter {
            key: "{tab_id}",
            excluded,
            available,
            onchange: move |action: ExtensionExclusionsAction| {
                let current = controller
                    .displayed(tab_id, fetched_epoch)
                    .unwrap_or_else(|| fallback.clone());
                let excluded = action.apply(&current);
                controller.submit(tab_id, FieldUpdate::Update(excluded.clone()), excluded);
            },
            onrestore: move |()| controller.submit(tab_id, FieldUpdate::Clear, defaults.clone()),
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
