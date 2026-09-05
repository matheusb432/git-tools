use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::LoadingSpinner;

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "grid size-12 place-items-center rounded-panel border border-acc-line bg-acc-soft text-acc",
            LoadingSpinner {}
        }
    }
}

/// Accent and inverse color contexts.
#[story(name = "Color contexts")]
fn color_contexts() -> Element {
    rsx! {
        div {
            class: "flex items-center gap-4",
            role: "status",
            aria_label: "Loading examples",
            div { class: "grid size-12 place-items-center rounded-panel border border-acc-line bg-acc-soft text-acc",
                LoadingSpinner {}
            }
            div { class: "grid size-12 place-items-center rounded-panel bg-acc text-bg",
                LoadingSpinner {}
            }
        }
    }
}

/// Loading spinner component.
#[stories(id = "loading-spinner", name = "Loading spinner", thumbnail = thumbnail)]
const LOADING_SPINNER_STORIES: () = &[color_contexts];
