use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::{
    app::application_navigation::ViewerTabDirection,
    shared::ui::{Button, ButtonSize, ButtonVariant, Checkbox},
    views::diffs::diff_workspace::review_actions::ReviewActionDock,
};

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "pointer-events-none", ReviewActionsDemo {} }
    }
}

#[story]
fn interactive() -> Element {
    rsx! {
        ReviewActionsDemo {}
    }
}

#[component]
fn ReviewActionsDemo() -> Element {
    let mut unpushed = use_signal(|| true);
    let mut collapsed = use_signal(|| false);
    let mut outcome = use_signal(|| "Review in progress");
    rsx! {
        div { class: "grid gap-4",
            div { class: "relative h-80 overflow-hidden border border-line bg-bg",
                div { class: "diff-document-scroll h-full px-5 pt-5 text-sm",
                    p { class: "mb-3 font-semibold text-ink", "src/review.rs" }
                    pre { class: "leading-7 text-add",
                        "+ review_snapshot();\n+ approve_changes();\n+ continue_review();"
                    }
                }
                ReviewActionDock {
                    unpushed: Some(unpushed()),
                    close_disabled: false,
                    tab_navigation_disabled: false,
                    collapsed: collapsed(),
                    onclose: move |_| outcome.set("Diff closed"),
                    onstep: move |direction| {
                        outcome
                            .set(
                                match direction {
                                    ViewerTabDirection::Next => "Next diff",
                                    ViewerTabDirection::Previous => "Previous diff",
                                },
                            );
                    },
                    oncollapsedchange: move |value| collapsed.set(value),
                    push: rsx! {
                        Button {
                            id: "review-push-trigger",
                            class: "review-push-action",
                            size: ButtonSize::Medium,
                            variant: ButtonVariant::Accent,
                            disabled: !unpushed(),
                            icon: rsx! {
                                lucide_dioxus::ArrowUp { size: 16 }
                            },
                            onclick: move |_| {
                                unpushed.set(false);
                                outcome.set("Push completed");
                            },
                            "Push"
                        }
                    },
                }
            }
            Checkbox {
                id: "preview-unpushed",
                label: "Snapshot has unpushed commits",
                checked: unpushed(),
                onchange: move |checked| unpushed.set(checked),
            }
            output { aria_live: "polite", class: "text-xs text-ink-3", "{outcome}" }
        }
    }
}

#[stories(id = "review-actions", name = "Review actions", thumbnail = thumbnail)]
const REVIEW_ACTIONS_STORIES: () = &[interactive];
