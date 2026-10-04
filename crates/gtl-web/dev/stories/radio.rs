use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::Radio;

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { role: "radiogroup", aria_label: "Diff layout",
            Radio {
                name: "story-radio-thumbnail",
                value: "unified",
                label: "Unified",
                checked: true,
                onchange: |()| {},
            }
            Radio {
                name: "story-radio-thumbnail",
                value: "split",
                label: "Split",
                checked: false,
                onchange: |()| {},
            }
        }
    }
}

/// Choices that keep their selection in local state.
#[story]
fn default() -> Element {
    let mut selected = use_signal(|| "unified");
    let options = [
        ("unified", "Unified"),
        ("split", "Split"),
        ("auto", "Match window width"),
    ];

    rsx! {
        div {
            class: "max-w-md",
            role: "radiogroup",
            aria_label: "Diff layout",
            for (value, label) in options {
                Radio {
                    key: "{value}",
                    name: "story-radio-layout",
                    value,
                    label,
                    checked: selected() == value,
                    onchange: move |()| selected.set(value),
                }
            }
        }
    }
}

/// Hints below each label describe their radio.
#[story]
fn hints() -> Element {
    let mut selected = use_signal(|| "markdown");
    let options = [
        (
            "markdown",
            "Markdown",
            "Locations, diff excerpts, and comments for an agent or pull request.",
        ),
        (
            "artifact",
            "HTML artifact",
            "The offline diff document with comments beneath their lines.",
        ),
    ];

    rsx! {
        div {
            class: "max-w-md",
            role: "radiogroup",
            aria_label: "Export format",
            for (value, label, hint) in options {
                Radio {
                    key: "{value}",
                    name: "story-radio-export",
                    value,
                    label,
                    hint,
                    checked: selected() == value,
                    onchange: move |()| selected.set(value),
                }
            }
        }
    }
}

/// Unavailable choice.
#[story]
fn disabled() -> Element {
    rsx! {
        div {
            class: "max-w-md",
            role: "radiogroup",
            aria_label: "Line density",
            Radio {
                name: "story-radio-density",
                value: "compact",
                label: "Compact",
                checked: true,
                onchange: |()| {},
            }
            Radio {
                name: "story-radio-density",
                value: "full",
                label: "Full file",
                hint: "Unavailable for pasted diff text.",
                checked: false,
                disabled: true,
                onchange: |()| {},
            }
        }
    }
}

/// Radio component.
#[stories(id = "radio", name = "Radio", thumbnail = thumbnail)]
const RADIO_STORIES: () = &[default, hints, disabled];
