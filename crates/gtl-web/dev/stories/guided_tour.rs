use dioxus::prelude::*;
use dx_story::{stories, story};
use gtl_models::settings::ViewerLanguage;

use crate::shared::{
    i18n::{t, use_language_provider},
    ui::{
        Button, ButtonSize, ButtonVariant, ScrollArea,
        guided_tour::{GuidedTour, GuidedTourAnchor, GuidedTourButton, GuidedTourStep},
    },
};

const FIRST: GuidedTourAnchor = GuidedTourAnchor::new("preview-tour-first");
const SECOND: GuidedTourAnchor = GuidedTourAnchor::new("preview-tour-second");
const HIDDEN: GuidedTourAnchor = GuidedTourAnchor::new("preview-tour-hidden");
const MISSING: GuidedTourAnchor = GuidedTourAnchor::new("preview-tour-missing");
const OFFSCREEN: GuidedTourAnchor = GuidedTourAnchor::new("preview-tour-offscreen");
const TOUR: GuidedTour = GuidedTour::new("preview-guided-tour", steps);

fn steps(language: ViewerLanguage) -> Vec<GuidedTourStep> {
    [FIRST, SECOND, HIDDEN, MISSING, OFFSCREEN, FIRST]
        .into_iter()
        .enumerate()
        .map(|(index, anchor)| {
            GuidedTourStep::new(
                anchor,
                t!(language, "tour-reading-1-title"),
                if index == 5 {
                    t!(language, "tour-reading-2-body").repeat(20)
                } else {
                    t!(language, "tour-reading-1-body")
                },
            )
        })
        .collect()
}

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "guided-tour-heading",
            "Screen guide"
            GuidedTourButton { tour: TOUR }
        }
    }
}

#[story]
fn interactive() -> Element {
    let mut language = use_signal(ViewerLanguage::default);
    use_language_provider(language.into());
    let mut light = use_signal(|| false);
    let palette = if light() {
        "--bg:#fff;--surface:#fff;--ink:#17212d;--ink-2:#45576b;--ink-3:#53657a;--line-2:#ccd6e0;--acc:#6940a5;--acc-2:#593690;color-scheme:light;background:var(--bg);color:var(--ink);"
    } else {
        "--bg:inherit;--surface:inherit;--ink:inherit;--ink-2:inherit;--ink-3:inherit;--line-2:inherit;--acc:inherit;--acc-2:inherit;color-scheme:inherit;background:var(--bg);color:var(--ink);"
    };
    rsx! {
        div { style: palette, class: "grid gap-4 p-4",
            div { class: "guided-tour-heading",
                h2 { "Guided tour" }
                GuidedTourButton { tour: TOUR }
            }
            div { class: "flex gap-3",
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    onclick: move |_| {
                        language
                            .set(
                                if language() == ViewerLanguage::EnUs {
                                    ViewerLanguage::PtBr
                                } else {
                                    ViewerLanguage::EnUs
                                },
                            );
                    },
                    if language() == ViewerLanguage::EnUs {
                        "Português"
                    } else {
                        "English"
                    }
                }
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    onclick: move |_| light.toggle(),
                    if light() {
                        "Use dark palette"
                    } else {
                        "Use light palette"
                    }
                }
            }
            div { hidden: true, "data-tour": FIRST.value(), "Hidden duplicate" }
            div {
                class: "rounded-panel border border-line p-3",
                "data-tour": FIRST.value(),
                "First target"
            }
            div {
                class: "ml-auto rounded-panel border border-line p-3",
                "data-tour": SECOND.value(),
                "Second target"
            }
            div {
                style: "visibility:hidden;height:20px",
                "data-tour": HIDDEN.value(),
                "Hidden target"
            }
            ScrollArea { style: "height:120px", class: "border border-line",
                div { style: "height:700px" }
                div { class: "p-3", "data-tour": OFFSCREEN.value(), "Target inside a scroll area" }
            }
        }
    }
}

#[stories(id = "guided-tour", name = "Guided tour", thumbnail = thumbnail)]
const GUIDED_TOUR_STORIES: () = &[interactive];
