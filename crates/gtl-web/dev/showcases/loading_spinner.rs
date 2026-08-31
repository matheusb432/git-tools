use dioxus::prelude::*;
use dx_preview::{preview, showcase};

use crate::shared::ui::LoadingSpinner;

#[preview(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "grid size-12 place-items-center rounded-panel border border-acc-line bg-acc-soft text-acc",
            LoadingSpinner {}
        }
    }
}

/// Accent and inverse color contexts.
#[preview(name = "Color contexts")]
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
#[showcase(id = "loading-spinner", name = "Loading spinner", thumbnail = thumbnail)]
const LOADING_SPINNER_SHOWCASE: () = &[color_contexts];
