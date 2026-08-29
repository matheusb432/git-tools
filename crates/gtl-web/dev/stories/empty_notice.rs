use dioxus::prelude::*;

use super::{StoryContext, StoryVariant};
use crate::shared::ui::EmptyNotice;

pub(super) const VARIANTS: &[StoryVariant] = &[
    StoryVariant {
        slug: "default",
        title: "Default",
        summary: "Default empty state.",
        render: default_notice,
    },
    StoryVariant {
        slug: "constrained",
        title: "Constrained",
        summary: "Narrow container.",
        render: constrained,
    },
];

pub(super) fn preview(_context: StoryContext) -> Element {
    rsx! {
        EmptyNotice { "No items found." }
    }
}

fn default_notice(_context: StoryContext) -> Element {
    rsx! {
        EmptyNotice { "No changed files match the current filter." }
    }
}

fn constrained(_context: StoryContext) -> Element {
    rsx! {
        div { class: "w-56",
            EmptyNotice { "No saved renders are available for this repository." }
        }
    }
}
