use dioxus::prelude::*;
use gtl_wire::viewer::VIEWER_SEARCH_QUERY_MAX_BYTES;

use super::{FileFiltersMenu, workspace::use_workspace_file_filters_context};
use crate::{
    app::application_layout::{ViewerContext, ViewerShellLoad},
    entities::diffs::viewer_server,
    shared::{
        browser,
        failure_notice::client_error_message,
        i18n::{t, use_language},
        ui::{Button, ButtonSize, ButtonVariant},
    },
    views::diffs::{
        changes_since::{ChangesSinceSelection, local_input_value},
        file_filter_changes::FileFilterController,
        file_filter_form::FileFilterEdit,
    },
};

/// Binds the filter menu to the active tab: saved extension filters and its session filters.
#[component]
pub(in crate::views::diffs::diff_workspace) fn WorkspaceFileFiltersMenu(id: String) -> Element {
    let language = use_language();
    let workspace = super::super::use_workspace_context();
    let filters = use_workspace_file_filters_context();
    let context = use_context::<ViewerContext>();
    let controller = use_context::<FileFilterController>();
    let identity = use_memo(move || workspace.view.read().identity);
    let refresh = use_memo(move || controller.refresh_epoch());
    let version = use_memo(move || match &*context.shell().read() {
        ViewerShellLoad::Ready(shell) => Some(shell.version),
        _ => None,
    });
    let mut metadata = use_resource(move || {
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
    let data = metadata.read();
    let fetched = data.as_ref().filter(|(id, _, _)| *id == tab_id);
    let fetched_epoch = fetched.map_or(0, |(_, epoch, _)| *epoch);
    let loaded = fetched.and_then(|(_, _, result)| result.as_ref().ok());
    let view = workspace.view.read();
    let applied = view.extension_filter.as_ref();
    let extension_filter = controller
        .displayed(tab_id, fetched_epoch)
        .or_else(|| loaded.map(|value| value.filter.clone()))
        .or_else(|| applied.map(|value| value.filter.clone()))
        .unwrap_or_default();
    let extensions = loaded
        .map(|value| value.extensions.clone())
        .unwrap_or_default();
    let error = controller
        .error(tab_id)
        .or_else(|| fetched.and_then(|(_, _, result)| result.as_ref().err().cloned()));
    let form = filters.form.read();
    let changes_since = filters.changes_since.applied.read();
    let changes_since_value = changes_since
        .as_ref()
        .map(|cutoff| local_input_value(cutoff, browser::local_offset_at(cutoff.instant())))
        .unwrap_or_default();
    rsx! {
        FileFiltersMenu {
            key: "{tab_id}",
            id,
            text: form.text.clone(),
            text_status: filters.text.state.read().message(language),
            text_maxlength: VIEWER_SEARCH_QUERY_MAX_BYTES.to_string(),
            ontext: move |text| filters.text.set_query.call(text),
            shown: form.shown,
            unreviewed: form.unreviewed,
            onunreviewed: move |checked| filters.edit.call(FileFilterEdit::Unreviewed(checked)),
            ontoggle: move |kind| filters.edit.call(FileFilterEdit::Toggle(kind)),
            changes_since: ChangesSinceSelection::new(changes_since.as_ref(), &form.changes_since),
            changes_since_value,
            changes_since_available: (filters.changes_since.available)(),
            onchangessince: move |edit| filters.changes_since.edit.call(edit),
            extension_filter,
            extensions,
            onextension: move |filter| controller.submit(tab_id, filter),
            hidden_count: (filters.hidden_count)(),
            active: (filters.active)(),
            onclear: move |()| filters.clear.call(()),
            if let Some(error) = error {
                div { class: "file-filters-error", role: "alert",
                    p { {client_error_message(&error, language)} }
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| {
                            controller.retry(tab_id);
                            metadata.restart();
                        },
                        {t!(language, "action-retry")}
                    }
                }
            }
        }
    }
}
