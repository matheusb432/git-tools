use dioxus::prelude::*;

use super::{CatalogConfig, CatalogRegistry, CatalogRoute, markdown::Markdown};
use crate::{Story, StoryVariant};

#[component]
pub(super) fn StoryView(story_id: String, variant_id: String) -> Element {
    let registry = use_context::<CatalogRegistry>();
    let Some((story, variant)) = registry.find(&story_id, &variant_id) else {
        return rsx! { MissingStory { story_id, variant_id } };
    };
    let mut generation = use_signal(|| 0_u64);
    let render_path = format!("/render/{}/{}", story.id(), variant.id());

    rsx! {
        section { class: "dsb-detail",
            header { class: "dsb-detail-header",
                StoryVariantPicker { story, selected_variant: variant }
                div { class: "dsb-detail-heading-row",
                    div { class: "dsb-detail-copy",
                        p { class: "dsb-eyebrow", "{story.name()}" }
                        h1 { "{variant.name()}" }
                        p { class: "dsb-detail-description",
                            {variant.description().unwrap_or("Component variant.")}
                        }
                    }
                    div { class: "dsb-detail-actions",
                        a {
                            class: "dsb-button dsb-button-secondary",
                            href: render_path,
                            target: "_blank",
                            rel: "noreferrer",
                            "Open canvas"
                        }
                        button {
                            class: "dsb-button",
                            r#type: "button",
                            onclick: move |_| *generation.write() += 1,
                            "Reset state"
                        }
                    }
                }
            }
            StoryCanvas { story, variant, generation: generation() }
            StoryDocumentation { story, variant }
        }
    }
}

#[component]
fn StoryVariantPicker(story: &'static Story, selected_variant: &'static StoryVariant) -> Element {
    let navigator = use_navigator();

    rsx! {
        label { class: "dsb-variant-picker",
            span { "Variant" }
            select {
                aria_label: "{story.name()} variant",
                value: selected_variant.id(),
                onchange: move |event| {
                    let selected_id = event.value();
                    let Some(next_variant) = story
                        .variants()
                        .iter()
                        .copied()
                        .find(|variant| variant.id() == selected_id)
                    else {
                        return;
                    };
                    if next_variant.id() != selected_variant.id() {
                        navigator.push(CatalogRoute::StoryView {
                            story_id: story.id().to_owned(),
                            variant_id: next_variant.id().to_owned(),
                        });
                    }
                },
                for variant in story.variants() {
                    option { key: "{variant.id()}", value: variant.id(), "{variant.name()}" }
                }
            }
        }
    }
}

#[component]
fn StoryCanvas(story: &'static Story, variant: &'static StoryVariant, generation: u64) -> Element {
    let config = use_context::<CatalogConfig>();
    let canvas_class = format!(
        "dsb-frame-content dsb-story-frame-content {}",
        config.canvas_class()
    );

    rsx! {
        div {
            class: "dsb-frame dsb-story-frame",
            "data-dioxus-storybook-ready": "true",
            "data-story": story.id(),
            "data-variant": variant.id(),
            div { class: canvas_class,
                for generation in [generation] {
                    StoryVariantRender {
                        key: "{story.id()}-{variant.id()}-{generation}",
                        variant,
                    }
                }
            }
        }
    }
}

#[component]
fn StoryVariantRender(variant: &'static StoryVariant) -> Element {
    (variant.render())()
}

#[component]
fn StoryDocumentation(story: &'static Story, variant: &'static StoryVariant) -> Element {
    rsx! {
        section { class: "dsb-documentation",
            div { class: "dsb-documentation-inner",
                if let Some(description) = story.description() {
                    article { class: "dsb-markdown",
                        h2 { "Documentation" }
                        Markdown { source: description }
                    }
                }
                details { class: "dsb-source",
                    summary { "Rust source" }
                    pre { code { "{variant.source()}" } }
                }
            }
        }
    }
}

#[component]
pub(super) fn RenderView(story_id: String, variant_id: String) -> Element {
    let config = use_context::<CatalogConfig>();
    let canvas_class = format!(
        "dsb-frame-content dsb-story-frame-content {}",
        config.canvas_class()
    );
    let registry = use_context::<CatalogRegistry>();
    let Some((story, variant)) = registry.find(&story_id, &variant_id) else {
        return rsx! { MissingStory { story_id, variant_id } };
    };
    let render = variant.render();

    rsx! {
        main {
            class: "dioxus-storybook dsb-frame dsb-render-frame",
            "data-dioxus-storybook-ready": "true",
            "data-story": story.id(),
            "data-variant": variant.id(),
            div { class: canvas_class, {render()} }
        }
    }
}

#[component]
fn MissingStory(story_id: String, variant_id: String) -> Element {
    rsx! {
        section { class: "dsb-message",
            p { class: "dsb-message-kicker dsb-message-kicker-danger", "Unknown story" }
            h1 { "{story_id}/{variant_id}" }
            p { "Choose a component from the storybook navigation." }
            Link { class: "dsb-button dsb-message-action", to: CatalogRoute::Home {},
                "Back to catalog"
            }
        }
    }
}
