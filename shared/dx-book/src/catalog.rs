//! Optional ready-to-run CSR catalog for registered component stories.

use std::num::NonZeroUsize;

use dioxus::prelude::*;

use crate::{Story, StoryRegistryError, StoryVariant, stories};

mod markdown;
mod pagination;
mod story;

use markdown::summary;
use pagination::CatalogPage;
use story::{RenderView, StoryView};

const CATALOG_STYLES: &str = include_str!("catalog/styles.css");

/// Default number of stories shown on one catalog page.
pub const DEFAULT_STORIES_PER_PAGE: NonZeroUsize = match NonZeroUsize::new(12) {
    Some(value) => value,
    None => NonZeroUsize::MIN,
};

/// Configuration for the feature-gated [`Storybook`] catalog.
#[derive(Clone, Copy, Debug)]
pub struct CatalogConfig {
    title: &'static str,
    stories_per_page: NonZeroUsize,
    canvas_class: &'static str,
    sidebar_footer: Option<fn() -> Element>,
}

impl CatalogConfig {
    /// Creates a catalog with the supplied application-specific title.
    #[must_use]
    pub const fn new(title: &'static str) -> Self {
        Self {
            title,
            stories_per_page: DEFAULT_STORIES_PER_PAGE,
            canvas_class: "",
            sidebar_footer: None,
        }
    }

    /// Sets the maximum number of cards shown on one catalog page.
    #[must_use]
    pub const fn with_stories_per_page(mut self, stories_per_page: NonZeroUsize) -> Self {
        self.stories_per_page = stories_per_page;
        self
    }

    /// Adds consumer-owned classes to every component canvas.
    ///
    /// Use this for application typography, theme scope, or other inherited
    /// presentation that production components normally receive from an app
    /// shell. Catalog chrome does not inspect or interpret the classes.
    #[must_use]
    pub const fn with_canvas_class(mut self, canvas_class: &'static str) -> Self {
        self.canvas_class = canvas_class;
        self
    }

    /// Adds an application-owned component to the bottom of the sidebar.
    ///
    /// The renderer executes inside a dedicated Dioxus component scope, so it
    /// may use hooks and application context.
    #[must_use]
    pub const fn with_sidebar_footer(mut self, render: fn() -> Element) -> Self {
        self.sidebar_footer = Some(render);
        self
    }

    const fn title(self) -> &'static str {
        self.title
    }

    const fn stories_per_page(self) -> NonZeroUsize {
        self.stories_per_page
    }

    pub(super) const fn canvas_class(self) -> &'static str {
        self.canvas_class
    }

    const fn sidebar_footer(self) -> Option<fn() -> Element> {
        self.sidebar_footer
    }
}

impl Default for CatalogConfig {
    fn default() -> Self {
        Self::new("Component storybook")
    }
}

impl PartialEq for CatalogConfig {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.stories_per_page == other.stories_per_page
            && self.canvas_class == other.canvas_class
            && optional_renderers_equal(self.sidebar_footer, other.sidebar_footer)
    }
}

impl Eq for CatalogConfig {}

fn optional_renderers_equal(left: Option<fn() -> Element>, right: Option<fn() -> Element>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => std::ptr::fn_addr_eq(left, right),
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

#[derive(Clone, Copy)]
pub(super) struct CatalogRegistry(&'static [&'static Story]);

impl CatalogRegistry {
    fn all(self) -> &'static [&'static Story] {
        self.0
    }

    pub(super) fn find(
        self,
        story_id: &str,
        variant_id: &str,
    ) -> Option<(&'static Story, &'static StoryVariant)> {
        let story = self
            .0
            .iter()
            .copied()
            .find(|story| story.id() == story_id)?;
        let variant = story
            .variants()
            .iter()
            .copied()
            .find(|variant| variant.id() == variant_id)?;
        Some((story, variant))
    }
}

#[derive(Clone, Debug, PartialEq, Routable)]
#[rustfmt::skip]
pub(super) enum CatalogRoute {
    #[layout(CatalogLayout)]
        #[route("/")]
        Home {},
        #[route("/story/:story_id/:variant_id")]
        StoryView { story_id: String, variant_id: String },
    #[end_layout]
    #[route("/render/:story_id/:variant_id")]
    RenderView { story_id: String, variant_id: String },
}

/// Renders the complete story catalog, router, navigation, and component canvas.
///
/// Application context and styles can be provided by wrapping this component in
/// a consumer-owned root component.
#[component]
pub fn Storybook(config: CatalogConfig) -> Element {
    use_context_provider(|| config);
    let registered_stories = match stories() {
        Ok(stories) => stories,
        Err(error) => {
            return rsx! {
                document::Style { {CATALOG_STYLES} }
                document::Title { "{config.title()}" }
                RegistryFailure { error }
            };
        }
    };
    use_context_provider(|| CatalogRegistry(registered_stories));

    rsx! {
        document::Style { {CATALOG_STYLES} }
        document::Title { "{config.title()}" }
        Router::<CatalogRoute> {}
    }
}

/// Validates registered stories and launches the default CSR catalog root.
///
/// Use [`Storybook`] from an application-owned root instead when stories need
/// context providers, fonts, stylesheets, or document metadata.
///
/// # Errors
///
/// Returns [`StoryRegistryError`] when the distributed story registry is invalid.
pub fn launch(config: CatalogConfig) -> Result<(), StoryRegistryError> {
    crate::init_story_registry()?;
    dioxus::LaunchBuilder::new()
        .with_context(config)
        .launch(launched_storybook);
    Ok(())
}

fn launched_storybook() -> Element {
    let config = use_context::<CatalogConfig>();
    rsx! { Storybook { config } }
}

#[component]
fn CatalogLayout() -> Element {
    let route = use_route::<CatalogRoute>();
    let config = use_context::<CatalogConfig>();
    let registry = use_context::<CatalogRegistry>();
    let active_story_id = match &route {
        CatalogRoute::StoryView { story_id, .. } => Some(story_id.as_str()),
        CatalogRoute::Home {} | CatalogRoute::RenderView { .. } => None,
    };

    rsx! {
        div { class: "dioxus-storybook",
            aside { class: "dsb-sidebar",
                header { class: "dsb-sidebar-header",
                    Link { class: "dsb-brand-link", to: CatalogRoute::Home {},
                        span { class: "dsb-brand-mark", aria_hidden: "true", "<>" }
                        span { "{config.title()}" }
                    }
                }
                nav { class: "dsb-story-nav dsb-story-nav-desktop", aria_label: "Component stories",
                    for story in registry.all() {
                        StoryNavigation {
                            key: "desktop-{story.id()}",
                            story: *story,
                            active: active_story_id == Some(story.id()),
                        }
                    }
                }
                SidebarFooter { class: "dsb-sidebar-footer dsb-sidebar-footer-desktop" }
                details { class: "dsb-mobile-menu",
                    summary { class: "dsb-mobile-menu-summary",
                        span { "Components" }
                        span { class: "dsb-mobile-menu-chevron", aria_hidden: "true", ">" }
                    }
                    nav { class: "dsb-story-nav dsb-story-nav-mobile", aria_label: "Mobile component stories",
                        for story in registry.all() {
                            StoryNavigation {
                                key: "mobile-{story.id()}",
                                story: *story,
                                active: active_story_id == Some(story.id()),
                            }
                        }
                    }
                    SidebarFooter { class: "dsb-sidebar-footer dsb-sidebar-footer-mobile" }
                }
            }
            main { class: "dsb-main", Outlet::<CatalogRoute> {} }
        }
    }
}

#[component]
fn SidebarFooter(class: &'static str) -> Element {
    let config = use_context::<CatalogConfig>();
    let Some(render) = config.sidebar_footer() else {
        return rsx! {};
    };
    rsx! { div { class, {render()} } }
}

#[component]
fn StoryNavigation(story: &'static Story, active: bool) -> Element {
    let Some(first_variant) = story.first_variant() else {
        return rsx! {};
    };

    rsx! {
        Link {
            class: if active { "dsb-story-link dsb-story-link-active" } else { "dsb-story-link" },
            to: CatalogRoute::StoryView {
                story_id: story.id().to_owned(),
                variant_id: first_variant.id().to_owned(),
            },
            aria_current: if active { "page" } else { "false" },
            "{story.name()}"
        }
    }
}

#[component]
fn Home() -> Element {
    let config = use_context::<CatalogConfig>();
    let registry = use_context::<CatalogRegistry>();
    let stories = registry.all();
    let mut page = use_signal(|| CatalogPage::first(stories.len(), config.stories_per_page()));
    let current_page = page();
    let visible_stories = &stories[current_page.item_range()];
    let previous_page = current_page.previous();
    let next_page = current_page.next();

    rsx! {
        section { class: "dsb-home",
            div { class: "dsb-home-inner",
                header { class: "dsb-home-header",
                    div {
                        p { class: "dsb-eyebrow", "Registered components" }
                        h1 { class: "dsb-home-title", "{config.title()}" }
                    }
                    CatalogPagination {
                        page_number: current_page.page_number(),
                        page_count: current_page.page_count(),
                        previous_disabled: previous_page.is_none(),
                        next_disabled: next_page.is_none(),
                        onprevious: move |()| {
                            if let Some(previous_page) = previous_page {
                                page.set(previous_page);
                            }
                        },
                        onnext: move |()| {
                            if let Some(next_page) = next_page {
                                page.set(next_page);
                            }
                        },
                    }
                }
                if stories.is_empty() {
                    div { class: "dsb-empty", role: "status",
                        strong { "No component stories are registered." }
                        span { "Compile at least one #[story] declaration into this target." }
                    }
                } else {
                    div { class: "dsb-card-grid",
                        for story in visible_stories {
                            StoryCard { key: "{story.id()}", story: *story }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CatalogPagination(
    page_number: NonZeroUsize,
    page_count: NonZeroUsize,
    previous_disabled: bool,
    next_disabled: bool,
    onprevious: EventHandler<()>,
    onnext: EventHandler<()>,
) -> Element {
    rsx! {
        nav { class: "dsb-pagination", aria_label: "Component catalog pages",
            button {
                class: "dsb-icon-button",
                r#type: "button",
                aria_label: "Previous component page",
                disabled: previous_disabled,
                onclick: move |_| onprevious.call(()),
                span { aria_hidden: "true", "<" }
            }
            output { class: "dsb-page-count", aria_live: "polite",
                "Page {page_number} of {page_count}"
            }
            button {
                class: "dsb-icon-button",
                r#type: "button",
                aria_label: "Next component page",
                disabled: next_disabled,
                onclick: move |_| onnext.call(()),
                span { aria_hidden: "true", ">" }
            }
        }
    }
}

#[component]
fn StoryCard(story: &'static Story) -> Element {
    let config = use_context::<CatalogConfig>();
    let canvas_class = format!("dsb-frame-content {}", config.canvas_class());
    let Some(first_variant) = story.first_variant() else {
        return rsx! {};
    };
    let Some(preview) = story.catalog_preview() else {
        return rsx! {};
    };
    let count = story.variants().len();
    let count_label = if count == 1 {
        "1 variant".to_owned()
    } else {
        format!("{count} variants")
    };
    let description = story.description().map_or_else(
        || "Component story.".to_owned(),
        |source| summary(source, 150),
    );
    let render = preview.render();

    rsx! {
        article { class: "dsb-card",
            Link {
                class: "dsb-card-link",
                to: CatalogRoute::StoryView {
                    story_id: story.id().to_owned(),
                    variant_id: first_variant.id().to_owned(),
                },
                span { class: "dsb-visually-hidden", "Open {story.name()} story" }
            }
            header { class: "dsb-card-header",
                div { class: "dsb-card-copy",
                    h2 { class: "dsb-card-title", "{story.name()}" }
                    p { class: "dsb-card-description", "{description}" }
                }
                span { class: "dsb-variant-count", aria_label: count_label,
                    span { aria_hidden: "true", "#" }
                    span { aria_hidden: "true", "{count}" }
                }
            }
            div {
                class: "dsb-frame dsb-card-frame",
                "data-dioxus-storybook-preview": story.id(),
                inert: true,
                aria_hidden: "true",
                div { class: canvas_class, {render()} }
            }
        }
    }
}

#[component]
fn RegistryFailure(error: StoryRegistryError) -> Element {
    rsx! {
        main { class: "dioxus-storybook dsb-message", role: "alert",
            p { class: "dsb-message-kicker dsb-message-kicker-danger", "Invalid story registry" }
            h1 { "Component preview unavailable" }
            p { "{error}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CatalogConfig, DEFAULT_STORIES_PER_PAGE};

    #[test]
    fn catalog_configuration_has_bounded_defaults() {
        let config = CatalogConfig::new("UI catalog");

        assert_eq!(config.title(), "UI catalog");
        assert_eq!(config.stories_per_page(), DEFAULT_STORIES_PER_PAGE);
        assert!(config.canvas_class().is_empty());
        assert!(config.sidebar_footer().is_none());
    }

    #[test]
    fn catalog_configuration_preserves_consumer_canvas_classes() {
        let config = CatalogConfig::new("UI catalog").with_canvas_class("font-sans app-theme");

        assert_eq!(config.canvas_class(), "font-sans app-theme");
    }
}
