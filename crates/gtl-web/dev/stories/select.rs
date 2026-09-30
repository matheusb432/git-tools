use dioxus::prelude::*;
use dx_story::{stories, story};
use lucide_dioxus::ListFilter;

use crate::shared::ui::{Select, SelectOption, select::SelectVariant};

fn layout_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("unified", "Unified"),
        SelectOption::new("split", "Side by side"),
        SelectOption::new("auto", "Match window width"),
    ]
}

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "max-w-56",
            Select {
                id: "story-select-thumbnail",
                aria_label: "Diff layout",
                value: "split",
                options: layout_options(),
                error: None,
            }
        }
    }
}

/// Field and toolbar variants that keep their value in local state.
#[story]
fn interactive() -> Element {
    let mut layout = use_signal(|| "split".to_owned());
    let mut status = use_signal(|| "active".to_owned());
    rsx! {
        div { class: "grid max-w-sm gap-6",
            Select {
                id: "story-select-layout",
                aria_label: "Diff layout",
                value: layout(),
                options: layout_options(),
                error: None,
                onchange: move |value| layout.set(value),
            }
            div { class: "w-40",
                Select {
                    id: "story-select-status",
                    aria_label: "Filter by status",
                    variant: SelectVariant::Toolbar,
                    value: status(),
                    options: vec![
                        SelectOption::new("active", "Active"),
                        SelectOption::new("paused", "Paused"),
                        SelectOption::new("all", "All"),
                    ],
                    error: None,
                    icon: rsx! {
                        ListFilter { size: 15 }
                    },
                    onchange: move |value| status.set(value),
                }
            }
            output { class: "text-xs text-ink-3", aria_live: "polite",
                "Layout: {layout} · Status: {status}"
            }
        }
    }
}

/// Invalid and disabled states.
#[story]
fn states() -> Element {
    rsx! {
        div { class: "grid max-w-sm gap-4",
            Select {
                id: "story-select-invalid",
                aria_label: "Theme",
                value: "",
                options: vec![SelectOption::new("", "Choose a theme")],
                error: "Choose a supported theme.",
            }
            Select {
                id: "story-select-disabled",
                aria_label: "Interface scale",
                value: "100",
                options: vec![SelectOption::new("100", "100%")],
                error: None,
                disabled: true,
            }
        }
    }
}

#[stories(id = "select", name = "Select", thumbnail = thumbnail)]
const SELECT_STORIES: () = &[interactive, states];
