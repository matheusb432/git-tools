use dioxus::prelude::*;

use super::{StoryContext, StoryVariant};
use crate::shared::ui::Skeleton;

pub(super) const VARIANTS: &[StoryVariant] = &[StoryVariant {
    slug: "content-stack",
    title: "Content stack",
    summary: "Text and control placeholders.",
    render: content_stack,
}];

pub(super) fn preview(_context: StoryContext) -> Element {
    rsx! {
        div { class: "grid w-full gap-3",
            div { class: "flex items-center gap-3",
                Skeleton { class: "size-10 rounded-full" }
                Skeleton { class: "h-3 flex-1" }
            }
            Skeleton { class: "h-3 w-full" }
            Skeleton { class: "h-3 w-3/4" }
        }
    }
}

fn content_stack(_context: StoryContext) -> Element {
    rsx! {
        div { class: "grid max-w-xl gap-3 rounded-panel border border-line bg-surface p-5",
            Skeleton { class: "h-4 w-36" }
            Skeleton { class: "h-3 w-full" }
            Skeleton { class: "h-3 w-4/5" }
            div { class: "mt-2 flex gap-2",
                Skeleton { class: "h-8 w-24" }
                Skeleton { class: "h-8 w-20" }
            }
        }
    }
}
