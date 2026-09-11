use dioxus::prelude::*;
use gtl_models::{live_views::LiveComparison, paths::RepositoryRoot};
use lucide_dioxus::{ArrowUp, FileDiff};

use super::status::ProjectSignal;
use crate::{
    app::application_router::Route,
    shared::ui::{ButtonLayout, ButtonSize, ButtonVariant, button_classes},
};
const SIGNAL_ROW_CLASSES: &str = "project-signal-row h-9 min-w-0 gap-3 px-3";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectActionShape {
    SignalRow,
    Icon,
}

#[component]
pub(super) fn ProjectComparisonAction(
    path: RepositoryRoot,
    comparison: LiveComparison,
    signal: ProjectSignal,
    label: &'static str,
    shape: ProjectActionShape,
    disabled: bool,
) -> Element {
    let available = signal.is_available() && !disabled;
    let classes = match shape {
        ProjectActionShape::SignalRow => SIGNAL_ROW_CLASSES.to_owned(),
        ProjectActionShape::Icon => button_classes(
            ButtonLayout::Inline,
            ButtonVariant::Ghost,
            ButtonSize::IconSmall,
        ),
    };
    let content = match shape {
        ProjectActionShape::SignalRow => rsx! {
            span {
                class: "project-signal-value font-medium tabular-nums {signal.text_classes()}",
                aria_hidden: "true",
                "{signal.glyph()}"
            }
            span { class: "whitespace-nowrap", "{label}" }
        },
        ProjectActionShape::Icon => rsx! {
            if comparison == LiveComparison::LocalChanges {
                FileDiff { size: 15 }
            } else {
                ArrowUp { size: 15 }
            }
        },
    };
    rsx! {
        if available {
            Link {
                to: Route::project_diff(&path, comparison),
                draggable: "false",
                class: classes,
                title: signal.description().into_owned(),
                aria_label: label,
                {content}
            }
        } else {
            button {
                r#type: "button",
                disabled: true,
                class: classes,
                title: signal.description().into_owned(),
                aria_label: label,
                {content}
            }
        }
    }
}
