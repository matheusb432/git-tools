use dioxus::prelude::*;
use lucide_dioxus::ChevronsDownUp;

use crate::shared::{
    browser,
    i18n::{t, use_language},
    ui::{Button, ButtonSize, ButtonVariant},
};

#[component]
pub(super) fn CollapseFilesButton() -> Element {
    let language = use_language();
    let workspace = super::use_workspace_context();

    let presentation = try_use_context::<crate::views::diffs::presentation::DiffPresentation>();
    let files_folded = (workspace.files_folded)().unwrap_or(false);
    let fold_label = if files_folded {
        t!(language, "files-expand-diffs")
    } else {
        t!(language, "files-collapse-diffs")
    };

    rsx! {
        Button {
            class: "mobile:size-11 mobile:p-0",
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            aria_label: fold_label.clone(),
            title: fold_label.clone(),

            onclick: move |_| {
                if let Some(presentation) = presentation {
                    presentation.toggle_files(workspace.view.peek().identity.tab_id);
                    return;
                }
                let folded = !files_folded;
                let mut folded_state = workspace.files_folded;
                folded_state.set(Some(folded));
                if folded {
                    browser::scroll_diff_document_to_start();
                }
            },
            span {
                class: "inline-flex flex-none mobile:[&_svg]:size-5",
                aria_hidden: "true",
                if files_folded {
                    lucide_dioxus::ChevronsUpDown { size: 14 }
                } else {
                    ChevronsDownUp { size: 14 }
                }
            }
            span { class: "sr-only", {fold_label} }
        }
    }
}
