use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::EmptyNotice;

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        EmptyNotice { "No items found." }
    }
}

/// Default empty state.
#[story]
fn default() -> Element {
    rsx! {
        EmptyNotice { "No changed files match the current filter." }
    }
}

/// Narrow container.
#[story]
fn constrained() -> Element {
    rsx! {
        div { class: "w-56",
            EmptyNotice { "No saved renders are available for this repository." }
        }
    }
}

/// Empty notice component.
#[stories(id = "empty-notice", name = "Empty notice", thumbnail = thumbnail)]
const EMPTY_NOTICE_STORIES: () = &[default, constrained];
