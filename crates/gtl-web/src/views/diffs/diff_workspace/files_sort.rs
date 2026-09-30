use dioxus::prelude::*;
use gtl_models::settings::DiffFilesSort;
use lucide_dioxus::ArrowDownWideNarrow;

use crate::{
    app::user_settings::UserSettings,
    shared::{
        browser,
        i18n::{t, use_language},
        ui::{IconPopover, Radio, popover::PopoverPlacement},
    },
    views::viewer_settings_form::SettingsEdit,
};

/// Chooses the order of the Files panel and the diff document.
#[component]
pub(crate) fn FilesSortMenu(
    id: String,
    sort: DiffFilesSort,
    onchange: EventHandler<DiffFilesSort>,
) -> Element {
    let language = use_language();
    let label = t!(language, "files-sort-selected", sort = sort.to_string());
    let options_name = format!("{id}-options");
    let heading_id = format!("{id}-heading");
    rsx! {
        IconPopover {
            id: id.clone(),
            aria_label: label,
            placement: PopoverPlacement::TriggerStart,
            icon: rsx! {
                ArrowDownWideNarrow { size: 16 }
            },
            div {
                class: "files-sort",
                role: "radiogroup",
                aria_labelledby: heading_id.clone(),
                p { id: heading_id, class: "files-sort-heading", {t!(language, "files-sort")} }
                for (value, option_label) in [
                    (DiffFilesSort::Path, t!(language, "files-sort-path")),
                    (DiffFilesSort::Changes, t!(language, "files-sort-changes")),
                ]
                {
                    Radio {
                        key: "{value}",
                        name: options_name.clone(),
                        value: value.to_string(),
                        label: option_label,
                        checked: sort == value,
                        onchange: {
                            let id = id.clone();
                            move |()| {
                                onchange.call(value);
                                browser::hide_popover(&id);
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Saves the chosen order as the `diff_files_sort` user setting.
#[component]
pub(super) fn SavedFilesSortMenu(id: String) -> Element {
    let settings = use_context::<UserSettings>();
    let workspace = super::use_workspace_context();
    rsx! {
        FilesSortMenu {
            id,
            sort: (workspace.files_sort)(),
            onchange: move |sort| settings.select.call(SettingsEdit::DiffFilesSort(sort)),
        }
    }
}
