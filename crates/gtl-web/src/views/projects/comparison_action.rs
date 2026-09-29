use dioxus::prelude::*;
use gtl_models::paths::RepositoryRoot;
use lucide_dioxus::FileText;

use crate::{
    app::application_router::Route,
    shared::{
        i18n::{t, use_language},
        ui::{ButtonLayout, ButtonSize, ButtonVariant, button_classes},
    },
};

/// Opens the project's unpushed or branch changes, or shows the tab that already has them.
#[component]
pub(super) fn ProjectComparisonAction(path: RepositoryRoot, disabled: bool) -> Element {
    let language = use_language();
    let label = t!(language, "projects-open-diff");
    let classes = button_classes(
        ButtonLayout::Inline,
        ButtonVariant::Ghost,
        ButtonSize::IconSmall,
    );
    let content = rsx! {
        FileText { size: 15 }
    };
    rsx! {
        if !disabled {
            Link {
                to: Route::project_diff(&path),
                draggable: "false",
                class: classes,
                title: label.clone(),
                aria_label: label,
                {content}
            }
        } else {
            button {
                r#type: "button",
                disabled: true,
                class: classes,
                title: label.clone(),
                aria_label: label,
                {content}
            }
        }
    }
}
