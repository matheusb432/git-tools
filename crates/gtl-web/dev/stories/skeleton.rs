use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::Skeleton;

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "grid w-full max-w-xs grid-cols-[2.5rem_1fr] items-center gap-3",
            Skeleton { class: "size-10 rounded-full" }
            div { class: "grid gap-2",
                Skeleton { class: "h-3 w-3/4" }
                Skeleton { class: "h-3 w-full" }
            }
        }
    }
}

/// Text and control placeholders.
#[story(name = "Content stack")]
fn content_stack() -> Element {
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

/// Loading placeholder component.
#[stories(id = "skeleton", name = "Skeleton", thumbnail = thumbnail)]
const SKELETON_STORIES: () = &[content_stack];
