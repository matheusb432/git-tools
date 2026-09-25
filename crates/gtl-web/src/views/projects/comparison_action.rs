use dioxus::prelude::*;
use gtl_models::paths::RepositoryRoot;
use gtl_wire::viewer::projects::ViewerProjectDiffMode;
use lucide_dioxus::{Activity, FileText};

use crate::{
    app::application_router::Route,
    shared::{
        i18n::{t, use_language},
        ui::{ButtonLayout, ButtonSize, ButtonVariant, button_classes},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectActionShape {
    Labeled,
    Icon,
}

#[component]
pub(super) fn ProjectComparisonAction(
    path: RepositoryRoot,
    mode: ViewerProjectDiffMode,
    shape: ProjectActionShape,
    disabled: bool,
) -> Element {
    let language = use_language();
    let label = match mode {
        ViewerProjectDiffMode::Snapshot => t!(language, "projects-create-snapshot"),
        ViewerProjectDiffMode::Live => t!(language, "projects-open-live"),
    };
    let classes = button_classes(
        ButtonLayout::Inline,
        if shape == ProjectActionShape::Icon {
            ButtonVariant::Ghost
        } else {
            ButtonVariant::Outline
        },
        if shape == ProjectActionShape::Icon {
            ButtonSize::IconSmall
        } else {
            ButtonSize::Small
        },
    );
    let content = rsx! {
        if mode == ViewerProjectDiffMode::Snapshot {
            FileText { size: 15 }
        } else {
            Activity { size: 15 }
        }
        if shape == ProjectActionShape::Labeled {
            span { class: "whitespace-nowrap", "{label}" }
        }
    };
    rsx! {
        if !disabled {
            Link {
                to: Route::project_diff(&path, mode),
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
