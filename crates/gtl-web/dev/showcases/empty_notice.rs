use dioxus::prelude::*;
use dx_preview::{preview, showcase};

use crate::shared::ui::EmptyNotice;

#[preview(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        EmptyNotice { "No items found." }
    }
}

/// Default empty state.
#[preview]
fn default() -> Element {
    rsx! {
        EmptyNotice { "No changed files match the current filter." }
    }
}

/// Narrow container.
#[preview]
fn constrained() -> Element {
    rsx! {
        div { class: "w-56",
            EmptyNotice { "No saved renders are available for this repository." }
        }
    }
}

/// Empty notice component.
#[showcase(id = "empty-notice", name = "Empty notice", thumbnail = thumbnail)]
const EMPTY_NOTICE_SHOWCASE: () = &[default, constrained];
