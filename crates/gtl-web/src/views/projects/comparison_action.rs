use dioxus::prelude::*;
use gtl_models::paths::RepositoryRoot;
use gtl_wire::viewer::projects::ViewerProjectDiffMode;
use lucide_dioxus::{Activity, FileText};

use crate::{
    app::application_router::Route,
    shared::ui::{ButtonLayout, ButtonSize, ButtonVariant, button_classes},
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
    unavailable_reason: Option<String>,
) -> Element {
    let label = match mode {
        ViewerProjectDiffMode::Snapshot => "Create snapshot",
        ViewerProjectDiffMode::Live => "Open live",
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
    let title = unavailable_reason.unwrap_or_else(|| label.to_owned());
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
                title,
                aria_label: label,
                {content}
            }
        } else {
            button {
                r#type: "button",
                disabled: true,
                class: classes,
                title,
                aria_label: label,
                {content}
            }
        }
    }
}
