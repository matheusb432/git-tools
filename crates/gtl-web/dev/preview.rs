use std::{num::NonZeroUsize, ops::Range};

use dioxus::prelude::*;
use gtl_wire::viewer::ViewerTheme;
use lucide_dioxus::{ChevronDown, Grid3X3};

use super::stories::{self, Story, StoryContext, StoryVariant};
use crate::shared::{
    browser,
    pagination::use_pagination,
    ui::{Button, ButtonSize, ButtonVariant, ViewerThemePicker},
};

const CATALOG_STORIES_PER_PAGE_MAX: usize = 15;
const FAVICON: Asset = asset!("/src/app/assets/app-icon.ico");
const PREVIEW_CSS: Asset = asset!("/assets/component-preview.css");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(GalleryLayout)]
        #[route("/")]
        Catalog {},
        #[route("/story/:story_slug/:variant_slug")]
        StoryView { story_slug: String, variant_slug: String },
    #[end_layout]
    #[route("/render/:story_slug/:variant_slug")]
    RenderView { story_slug: String, variant_slug: String },
}

#[component]
pub(super) fn App() -> Element {
    let theme = use_signal(|| ViewerTheme::Dark);
    use_context_provider(|| theme);
    let selected_theme = theme();
    use_effect(use_reactive((&selected_theme,), move |(selected_theme,)| {
        browser::apply_theme(selected_theme.as_str());
    }));

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: PREVIEW_CSS, blocking: "render" }
        document::Title { "Component storybook" }
        Router::<Route> {}
    }
}

#[component]
fn GalleryLayout() -> Element {
    let route = use_route::<Route>();
    let mut theme = use_context::<Signal<ViewerTheme>>();
    let active_story_slug = match &route {
        Route::StoryView { story_slug, .. } => Some(story_slug.as_str()),
        Route::Catalog {} | Route::RenderView { .. } => None,
    };

    rsx! {
        div {
            class: "min-h-screen bg-storybook-bg font-mono text-storybook-ink md:grid md:grid-cols-[18.75rem_minmax(0,1fr)]",
            style: "color-scheme: dark;",
            aside { class: "border-b border-storybook-line bg-storybook-surface md:sticky md:top-0 md:flex md:h-screen md:flex-col md:border-r md:border-b-0",
                header { class: "border-b border-storybook-line px-5 py-6 md:px-6 md:pt-10 md:pb-4",
                    Link {
                        class: "text-lg font-semibold tracking-tight text-storybook-ink no-underline focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-storybook-action",
                        to: Route::Catalog {},
                        "Component storybook"
                    }
                }
                nav {
                    class: "hidden flex-1 space-y-1 overflow-y-auto px-3 py-4 md:block",
                    aria_label: "Component stories",
                    for story in stories::all() {
                        StoryNavigation {
                            key: "desktop-{story.slug}",
                            story: *story,
                            active: active_story_slug == Some(story.slug),
                        }
                    }
                }
                div { class: "hidden border-t border-storybook-line px-5 pt-5 pb-11 md:block",
                    ViewerThemePicker {
                        theme: theme(),
                        disabled: false,
                        onthemechange: move |selected_theme| theme.set(selected_theme),
                    }
                }
                details { class: "group md:hidden",
                    summary { class: "flex cursor-pointer list-none items-center justify-between px-5 py-3 text-sm text-storybook-ink-muted focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-storybook-action",
                        span { "Components" }
                        span {
                            class: "-rotate-90 transition-transform group-open:rotate-0",
                            aria_hidden: "true",
                            ChevronDown { size: 16 }
                        }
                    }
                    nav {
                        class: "space-y-1 border-t border-storybook-line px-3 py-4",
                        aria_label: "Mobile component stories",
                        for story in stories::all() {
                            StoryNavigation {
                                key: "mobile-{story.slug}",
                                story: *story,
                                active: active_story_slug == Some(story.slug),
                            }
                        }
                    }
                    div { class: "border-t border-storybook-line px-5 py-5",
                        ViewerThemePicker {
                            theme: theme(),
                            disabled: false,
                            onthemechange: move |selected_theme| theme.set(selected_theme),
                        }
                    }
                }
            }
            main { class: "min-w-0", Outlet::<Route> {} }
        }
    }
}

#[component]
fn StoryNavigation(story: Story, active: bool) -> Element {
    let Some(first_variant) = story.variants.first() else {
        return rsx! {};
    };
    let link_class = if active {
        "block min-w-0 rounded-sm border-l-2 border-storybook-action bg-storybook-surface-selected px-4 py-3 text-sm text-storybook-ink no-underline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-storybook-action"
    } else {
        "block min-w-0 rounded-sm border-l-2 border-transparent px-4 py-3 text-sm text-storybook-ink-muted no-underline hover:bg-storybook-surface-raised hover:text-storybook-ink focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-storybook-action"
    };

    rsx! {
        Link {
            class: link_class,
            to: Route::StoryView {
                story_slug: story.slug.to_owned(),
                variant_slug: first_variant.slug.to_owned(),
            },
            aria_current: if active { "page" } else { "false" },
            "{story.title}"
        }
    }
}

#[component]
fn Catalog() -> Element {
    let stories = stories::all();
    let page_size = NonZeroUsize::new(CATALOG_STORIES_PER_PAGE_MAX).unwrap_or(NonZeroUsize::MIN);
    let pagination = use_pagination(stories.len(), page_size);

    rsx! {
        section { class: "min-h-screen px-5 py-8 sm:px-8 lg:px-9 lg:py-10",
            div { class: "mx-auto w-full max-w-[90rem]",
                header { class: "flex items-center justify-between gap-4",
                    h1 { class: "text-2xl font-semibold tracking-tight text-storybook-ink md:text-[1.75rem]",
                        "Component storybook"
                    }
                    CatalogPageControls {
                        previous_page_number: pagination.previous_page_number,
                        next_page_number: pagination.next_page_number,
                        onprevious: pagination.on_previous,
                        onnext: pagination.on_next,
                    }
                }
                CatalogStoryGrid { item_range: pagination.item_range }
            }
        }
    }
}

#[component]
fn CatalogStoryGrid(item_range: ReadSignal<Range<usize>>) -> Element {
    let stories = stories::all();
    let visible_stories = &stories[item_range()];

    rsx! {
        div { class: "mt-7 flex flex-wrap items-stretch justify-center gap-4",
            for story in visible_stories {
                StoryCard { key: "{story.slug}", story: *story }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogPageDirection {
    Previous,
    Next,
}

impl CatalogPageDirection {
    const fn label(self) -> &'static str {
        match self {
            Self::Previous => "Previous component page",
            Self::Next => "Next component page",
        }
    }

    const fn symbol(self) -> &'static str {
        match self {
            Self::Previous => "<",
            Self::Next => ">",
        }
    }
}

#[component]
fn CatalogPageControls(
    previous_page_number: ReadSignal<Option<NonZeroUsize>>,
    next_page_number: ReadSignal<Option<NonZeroUsize>>,
    onprevious: EventHandler<()>,
    onnext: EventHandler<()>,
) -> Element {
    rsx! {
        nav {
            class: "flex items-center gap-1",
            aria_label: "Component catalog pages",
            CatalogPageButton {
                direction: CatalogPageDirection::Previous,
                target_page_number: previous_page_number,
                onactivate: move |()| onprevious.call(()),
            }
            CatalogPageButton {
                direction: CatalogPageDirection::Next,
                target_page_number: next_page_number,
                onactivate: move |()| onnext.call(()),
            }
        }
    }
}

#[component]
fn CatalogPageButton(
    direction: CatalogPageDirection,
    target_page_number: ReadSignal<Option<NonZeroUsize>>,
    onactivate: EventHandler<()>,
) -> Element {
    let disabled = target_page_number().is_none();

    rsx! {
        button {
            class: "grid size-8 cursor-pointer place-items-center rounded-sm text-storybook-ink-muted hover:bg-storybook-surface-selected hover:text-storybook-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-storybook-action disabled:cursor-not-allowed disabled:opacity-40",
            r#type: "button",
            aria_label: direction.label(),
            disabled,
            onclick: move |_| onactivate.call(()),
            span { aria_hidden: "true", "{direction.symbol()}" }
        }
    }
}

#[component]
fn StoryCard(story: Story) -> Element {
    let theme = use_context::<Signal<ViewerTheme>>();
    let Some(first_variant) = story.variants.first() else {
        return rsx! {};
    };
    let preview = story.preview;
    let render = preview.render;
    let context = StoryContext {
        reset_generation: 0,
    };
    let count = story.variants.len();
    let count_label = if count == 1 {
        "1 variant".to_owned()
    } else {
        format!("{count} variants")
    };

    rsx! {
        article { class: "group relative grid min-h-[18rem] w-full max-w-[22.5rem] cursor-pointer grid-rows-[auto_minmax(10rem,1fr)] rounded-panel border border-storybook-line bg-storybook-surface-raised p-5 hover:border-storybook-line-strong focus-within:border-storybook-action",
            Link {
                class: "absolute inset-0 z-10 rounded-panel focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-storybook-action",
                to: Route::StoryView {
                    story_slug: story.slug.to_owned(),
                    variant_slug: first_variant.slug.to_owned(),
                },
                span { class: "sr-only", "Open {story.title} story" }
            }
            header { class: "grid grid-cols-[minmax(0,1fr)_auto] items-start gap-4 pb-8",
                div { class: "min-w-0",
                    h2 { class: "text-base font-semibold text-storybook-ink", "{story.title}" }
                    p { class: "mt-1 text-xs leading-5 text-storybook-ink-subtle",
                        "{story.summary}"
                    }
                }
                span {
                    class: "inline-flex h-7 items-center gap-1.5 rounded-sm border border-storybook-line bg-storybook-bg px-2 text-xs text-storybook-ink-subtle",
                    aria_label: count_label,
                    span { aria_hidden: "true",
                        Grid3X3 { size: 13 }
                    }
                    span { aria_hidden: "true", "{count}" }
                }
            }
            div {
                class: "pointer-events-none grid min-h-40 place-items-center overflow-hidden rounded-sm border border-storybook-line bg-storybook-surface p-5 text-ink select-none",
                "data-theme": theme().as_str(),
                "inert": true,
                aria_hidden: "true",
                div { class: "w-full max-w-xl", {render(context)} }
            }
        }
    }
}

#[component]
fn StoryView(story_slug: String, variant_slug: String) -> Element {
    let theme = use_context::<Signal<ViewerTheme>>();
    let Some((story, variant)) = stories::find(&story_slug, &variant_slug) else {
        return rsx! {
            MissingStory { story_slug, variant_slug }
        };
    };
    let mut generation = use_signal(|| 0_u64);
    let render_path = format!("/render/{}/{}", story.slug, variant.slug);

    rsx! {
        section { class: "grid min-h-screen grid-rows-[auto_minmax(0,1fr)] bg-storybook-bg",
            header {
                class: "grid gap-4 border-b border-storybook-line bg-storybook-surface px-5 py-5 md:px-8",
                "data-theme": theme().as_str(),
                StoryVariantPicker { story: *story, selected_variant: *variant }
                div { class: "flex flex-wrap items-end justify-between gap-4",
                    div { class: "min-w-0",
                        p { class: "text-xs font-semibold text-storybook-action",
                            "{story.title}"
                        }
                        h1 { class: "mt-1 text-xl font-semibold tracking-tight text-storybook-ink",
                            "{variant.title}"
                        }
                        p { class: "mt-1 max-w-2xl text-sm leading-5 text-storybook-ink-subtle",
                            "{variant.summary}"
                        }
                    }
                    div { class: "flex flex-wrap items-end gap-2",
                        OpenCanvasLink { href: render_path }
                        Button {
                            size: ButtonSize::Medium,
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| *generation.write() += 1,
                            "Reset state"
                        }
                    }
                }
            }
            StoryCanvas { story: *story, variant: *variant, generation: generation() }
        }
    }
}

#[component]
fn OpenCanvasLink(href: String) -> Element {
    rsx! {
        a {
            class: "inline-flex h-9 items-center justify-center rounded-sm border border-storybook-line bg-storybook-surface-raised px-4 text-sm text-storybook-ink no-underline hover:border-storybook-line-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-storybook-action",
            href,
            target: "_blank",
            rel: "noreferrer",
            "Open canvas"
        }
    }
}

#[component]
fn StoryVariantPicker(story: Story, selected_variant: StoryVariant) -> Element {
    let navigator = use_navigator();

    rsx! {
        label { class: "grid w-48 gap-1 text-xs text-storybook-ink-subtle",
            span { "Variant" }
            select {
                class: "h-9 w-full rounded-sm border border-storybook-line bg-storybook-bg px-3 text-sm text-storybook-ink outline-none hover:border-storybook-line-strong focus-visible:border-storybook-action focus-visible:ring-2 focus-visible:ring-storybook-action-soft",
                aria_label: "{story.title} variant",
                value: selected_variant.slug,
                onchange: move |event| {
                    let selected_slug = event.value();
                    let Some(next_variant) = story
                        .variants
                        .iter()
                        .find(|variant| variant.slug == selected_slug)
                        else {
                        return;
                    };
                    if next_variant.slug != selected_variant.slug {
                        navigator
                            .push(Route::StoryView {
                                story_slug: story.slug.to_owned(),
                                variant_slug: next_variant.slug.to_owned(),
                            });
                    }
                },
                for variant in story.variants {
                    option { key: "{variant.slug}", value: variant.slug, "{variant.title}" }
                }
            }
        }
    }
}

#[component]
fn StoryCanvas(story: Story, variant: StoryVariant, generation: u64) -> Element {
    let theme = use_context::<Signal<ViewerTheme>>();
    let render = variant.render;
    let context = StoryContext {
        reset_generation: generation,
    };
    rsx! {
        div {
            class: "grid min-h-[26rem] place-items-center overflow-auto bg-bg p-5 text-ink md:p-10",
            "data-theme": theme().as_str(),
            "data-gtl-component-preview-ready": "true",
            "data-story": story.slug,
            "data-variant": variant.slug,
            div { class: "w-full max-w-3xl", {render(context)} }
        }
    }
}

#[component]
fn RenderView(story_slug: String, variant_slug: String) -> Element {
    let theme = use_context::<Signal<ViewerTheme>>();
    let Some((story, variant)) = stories::find(&story_slug, &variant_slug) else {
        return rsx! {
            MissingStory { story_slug, variant_slug }
        };
    };
    let render = variant.render;
    let context = StoryContext {
        reset_generation: 0,
    };

    rsx! {
        main {
            class: "grid min-h-screen place-items-center overflow-auto bg-bg p-5 text-ink md:p-10",
            "data-theme": theme().as_str(),
            "data-gtl-component-preview-ready": "true",
            "data-story": story.slug,
            "data-variant": variant.slug,
            div { class: "w-full max-w-3xl", {render(context)} }
        }
    }
}

#[component]
fn MissingStory(story_slug: String, variant_slug: String) -> Element {
    rsx! {
        section { class: "mx-auto grid min-h-screen max-w-xl place-content-center gap-3 px-5 text-center text-storybook-ink",
            p { class: "text-xs font-semibold text-storybook-danger", "Unknown story" }
            h1 { class: "text-xl font-semibold", "{story_slug}/{variant_slug}" }
            p { class: "text-sm text-storybook-ink-subtle",
                "Choose a component from the storybook navigation."
            }
            Link {
                class: "mx-auto mt-2 inline-flex h-9 items-center rounded-sm border border-storybook-line-strong bg-storybook-surface-raised px-4 text-sm text-storybook-ink no-underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-storybook-action",
                to: Route::Catalog {},
                "Back to catalog"
            }
        }
    }
}
