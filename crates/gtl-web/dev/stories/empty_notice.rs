use dioxus::prelude::*;
use dx_book::{story, variant};

use crate::shared::ui::EmptyNotice;

#[variant(name = "Catalog preview")]
fn preview() -> Element {
    rsx! {
        EmptyNotice { "No items found." }
    }
}

/// Default empty state.
#[variant]
fn default() -> Element {
    rsx! {
        EmptyNotice { "No changed files match the current filter." }
    }
}

/// Narrow container.
#[variant]
fn constrained() -> Element {
    rsx! {
        div { class: "w-56",
            EmptyNotice { "No saved renders are available for this repository." }
        }
    }
}

/// Empty notice component.
#[story(id = "empty-notice", name = "Empty notice", preview = preview)]
const EMPTY_NOTICE_STORY: () = &[default, constrained];
