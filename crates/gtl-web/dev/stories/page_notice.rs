use dioxus::prelude::*;
use dx_story::{stories, story};
use lucide_dioxus::FileDiff;

use crate::shared::ui::{Button, ButtonVariant, PageNotice};

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        PageNotice {
            class: "min-h-32 px-5",
            title: "No diff is open",
            message: "Open a diff to begin.",
            icon: rsx! {
                FileDiff { size: 22 }
            },
        }
    }
}

/// Empty state with a leading icon.
#[story]
fn empty() -> Element {
    rsx! {
        PageNotice {
            class: "min-h-64 px-5",
            title: "No diff is open",
            message: "Run a git-tools diff command to open a snapshot or live view.",
            icon: rsx! {
                FileDiff { size: 22 }
            },
        }
    }
}

/// Recoverable error state.
#[story]
fn error() -> Element {
    rsx! {
        PageNotice {
            class: "min-h-64 px-5",
            role: "alert",
            title: "Viewer state is unavailable",
            message: "The viewer service is not responding.",
            Button { class: "mx-auto mt-4", variant: ButtonVariant::Outline, "Try again" }
        }
    }
}

/// Page notice component.
#[stories(id = "page-notice", name = "Page notice", thumbnail = thumbnail)]
const PAGE_NOTICE_STORIES: () = &[empty, error];
