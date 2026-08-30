//! CSR component stories for Dioxus applications.
//!
//! The declaration API follows Holt Book: [`variant`] turns a zero-argument
//! render function into a story variant, and [`story`] groups variants into an
//! automatically registered story. dx-book additionally supports
//! stable variant metadata and an optional compact catalog preview.
//!
//! The default feature set contains only registration, lookup, and macros. Add
//! `csr` for the validated generic [`launch`] helper. Add `catalog` for the
//! complete CSR router, responsive sidebar, paginated card grid, story canvas,
//! and Markdown documentation renderer. The catalog feature includes `csr`.
//!
//! Applications whose stories need context or product styles can wrap
//! [`catalog::Storybook`] in their own root. Applications without additional
//! bootstrap requirements can call [`catalog::launch`] directly.
//!
//! ```
//! use dioxus::prelude::*;
//! use dx_book::{story, variant};
//!
//! #[variant]
//! fn default() -> Element {
//!     rsx! { button { "Save" } }
//! }
//!
//! #[variant(name = "Catalog preview")]
//! fn preview() -> Element {
//!     rsx! { button { "Save" } }
//! }
//!
//! /// A button component.
//! #[story(id = "button", name = "Button", preview = preview)]
//! const BUTTON_STORY: () = &[default];
//! ```

extern crate self as dx_book;

use std::{collections::HashSet, error::Error, fmt, sync::OnceLock};

use dioxus::prelude::Element;
pub use dx_book_macros::{story, variant};

#[cfg(feature = "catalog")]
pub mod catalog;

/// Implementation details used by code generated from [`story`] and [`variant`].
#[doc(hidden)]
pub mod __private {
    pub use const_format::concatcp;
    pub use dioxus;
    pub use inventory::submit;

    #[doc(hidden)]
    #[must_use]
    pub const fn story_variant(
        id: &'static str,
        name: &'static str,
        description: Option<&'static str>,
        render: fn() -> dioxus::prelude::Element,
        source: &'static str,
    ) -> super::StoryVariant {
        super::StoryVariant {
            id,
            name,
            description,
            render,
            source,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn story(
        id: &'static str,
        name: &'static str,
        description: Option<&'static str>,
        preview: Option<&'static super::StoryVariant>,
        variants: &'static [&'static super::StoryVariant],
    ) -> super::Story {
        super::Story {
            id,
            name,
            description,
            preview,
            variants,
        }
    }

    #[cfg(feature = "benchmark-support")]
    #[doc(hidden)]
    pub fn build_registry_for_benchmark(
        registrations: impl IntoIterator<Item = &'static super::Story>,
    ) -> Result<Box<[&'static super::Story]>, super::StoryRegistryError> {
        super::build_registry(registrations)
    }
}

/// One renderable state of a component story.
#[derive(Clone, Copy, Debug)]
pub struct StoryVariant {
    /// Stable URL segment for this variant.
    id: &'static str,
    /// Display name shown by the storybook.
    name: &'static str,
    /// Optional Markdown documentation taken from the variant function.
    description: Option<&'static str>,
    /// Creates a Dioxus component node that owns this variant's hooks.
    render: fn() -> Element,
    /// Rust source captured from the annotated variant function.
    source: &'static str,
}

impl StoryVariant {
    /// Returns the stable URL segment for this variant.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// Returns the display name shown by storybook catalogs.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the optional Markdown documentation captured from the variant.
    #[must_use]
    pub const fn description(&self) -> Option<&'static str> {
        self.description
    }

    /// Returns the component renderer generated for this variant.
    #[must_use]
    pub const fn render(&self) -> fn() -> Element {
        self.render
    }

    /// Returns the Rust source captured from the annotated variant function.
    #[must_use]
    pub const fn source(&self) -> &'static str {
        self.source
    }
}

impl PartialEq for StoryVariant {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.description == other.description
            && std::ptr::fn_addr_eq(self.render, other.render)
            && self.source == other.source
    }
}

impl Eq for StoryVariant {}

/// A named group of component variants.
#[derive(Clone, Copy, Debug)]
pub struct Story {
    /// Stable identifier used in story routes.
    id: &'static str,
    /// Display name shown by the storybook.
    name: &'static str,
    /// Optional Markdown documentation taken from the story declaration.
    description: Option<&'static str>,
    /// Optional compact renderer intended for inert catalog cards.
    preview: Option<&'static StoryVariant>,
    /// Routed, interactive variants registered for this story.
    variants: &'static [&'static StoryVariant],
}

impl Story {
    /// Returns the stable identifier used in story routes.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// Returns the display name shown by storybook catalogs.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the optional Markdown documentation captured from the story.
    #[must_use]
    pub const fn description(&self) -> Option<&'static str> {
        self.description
    }

    /// Returns the explicitly selected compact catalog preview.
    #[must_use]
    pub const fn preview(&self) -> Option<&'static StoryVariant> {
        self.preview
    }

    /// Returns the routed, interactive variants registered for this story.
    #[must_use]
    pub const fn variants(&self) -> &'static [&'static StoryVariant] {
        self.variants
    }

    /// Returns the first routed variant.
    #[must_use]
    pub fn first_variant(&self) -> Option<&'static StoryVariant> {
        self.variants.first().copied()
    }

    /// Returns the explicit catalog preview, or the first routed variant when
    /// the Holt-compatible declaration omitted one.
    #[must_use]
    pub fn catalog_preview(&self) -> Option<&'static StoryVariant> {
        self.preview.or_else(|| self.first_variant())
    }
}

impl PartialEq for Story {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.description == other.description
            && self.preview == other.preview
            && self.variants == other.variants
    }
}

impl Eq for Story {}

/// A deterministic registry construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryRegistryError {
    /// More than one distributed registration used the same story ID.
    DuplicateStoryId {
        /// The repeated story ID.
        id: &'static str,
    },
    /// A story registered more than one routed variant with the same ID.
    DuplicateVariantId {
        /// The story containing the repeated variant.
        story_id: &'static str,
        /// The repeated variant ID.
        variant_id: &'static str,
    },
    /// A story did not register any routed variants.
    StoryWithoutVariants {
        /// The story without a route target.
        story_id: &'static str,
    },
}

impl fmt::Display for StoryRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateStoryId { id } => {
                write!(formatter, "duplicate story ID `{id}`")
            }
            Self::DuplicateVariantId {
                story_id,
                variant_id,
            } => write!(
                formatter,
                "duplicate variant ID `{variant_id}` in story `{story_id}`"
            ),
            Self::StoryWithoutVariants { story_id } => {
                write!(formatter, "story `{story_id}` has no routed variants")
            }
        }
    }
}

impl Error for StoryRegistryError {}

inventory::collect!(&'static Story);

static STORY_REGISTRY: OnceLock<Result<Box<[&'static Story]>, StoryRegistryError>> =
    OnceLock::new();
const STORY_COUNT_LINEAR_VALIDATION_MAX: usize = 16;

#[cfg(target_family = "wasm")]
unsafe extern "C" {
    fn __wasm_call_ctors();
}

/// Initializes distributed story registrations for the current WebAssembly
/// instance and validates their stable IDs.
///
/// # Errors
///
/// Returns [`StoryRegistryError`] when registered story or variant IDs are not
/// unique or a story has no routed variants.
pub fn init_story_registry() -> Result<(), StoryRegistryError> {
    registry().map(|_| ())
}

/// Returns every registered story in stable display-name order.
///
/// # Errors
///
/// Returns [`StoryRegistryError`] when registered story or variant IDs are not
/// unique or a story has no routed variants.
pub fn stories() -> Result<&'static [&'static Story], StoryRegistryError> {
    registry()
}

/// Finds a registered story and variant by their stable identifiers.
///
/// # Errors
///
/// Returns [`StoryRegistryError`] when registered story or variant IDs are not
/// unique or a story has no routed variants.
pub fn find(
    story_id: &str,
    variant_id: &str,
) -> Result<Option<(&'static Story, &'static StoryVariant)>, StoryRegistryError> {
    let story = stories()?
        .iter()
        .copied()
        .find(|story| story.id() == story_id);
    let Some(story) = story else {
        return Ok(None);
    };
    let variant = story
        .variants()
        .iter()
        .copied()
        .find(|variant| variant.id() == variant_id);
    Ok(variant.map(|variant| (story, variant)))
}

/// Validates story registration and launches a Dioxus CSR root component.
///
/// # Errors
///
/// Returns [`StoryRegistryError`] instead of launching when registered story or
/// variant IDs are not unique or a story has no routed variants.
#[cfg(feature = "csr")]
pub fn launch(root: fn() -> Element) -> Result<(), StoryRegistryError> {
    init_story_registry()?;
    dioxus::launch(root);
    Ok(())
}

fn registry() -> Result<&'static [&'static Story], StoryRegistryError> {
    STORY_REGISTRY
        .get_or_init(|| {
            initialize_distributed_registrations();
            build_registry(inventory::iter::<&'static Story>.into_iter().copied())
        })
        .as_ref()
        .map(Box::as_ref)
        .map_err(|error| *error)
}

fn initialize_distributed_registrations() {
    #[cfg(target_family = "wasm")]
    // SAFETY: Wasm synthesizes this function for static constructors. OnceLock
    // runs this initializer exactly once before inventory is read.
    unsafe {
        __wasm_call_ctors();
    }
}

fn build_registry(
    registrations: impl IntoIterator<Item = &'static Story>,
) -> Result<Box<[&'static Story]>, StoryRegistryError> {
    let mut stories = registrations.into_iter().collect::<Vec<_>>();
    stories.sort_unstable_by_key(|story| (story.name(), story.id()));
    validate_unique_ids(&stories)?;
    Ok(stories.into_boxed_slice())
}

fn validate_unique_ids(stories: &[&'static Story]) -> Result<(), StoryRegistryError> {
    if stories.len() > STORY_COUNT_LINEAR_VALIDATION_MAX {
        return validate_unique_ids_with_hash_set(stories);
    }

    validate_unique_ids_with_linear_scan(stories)
}

fn validate_unique_ids_with_linear_scan(
    stories: &[&'static Story],
) -> Result<(), StoryRegistryError> {
    for (index, story) in stories.iter().enumerate() {
        if stories[..index]
            .iter()
            .any(|registered| registered.id() == story.id())
        {
            return Err(StoryRegistryError::DuplicateStoryId { id: story.id() });
        }
        if story.variants().is_empty() {
            return Err(StoryRegistryError::StoryWithoutVariants {
                story_id: story.id(),
            });
        }
        if let Some(variant_id) = duplicate_variant_id(story) {
            return Err(StoryRegistryError::DuplicateVariantId {
                story_id: story.id(),
                variant_id,
            });
        }
    }
    Ok(())
}

fn validate_unique_ids_with_hash_set(stories: &[&'static Story]) -> Result<(), StoryRegistryError> {
    let mut story_ids = HashSet::with_capacity(stories.len());
    for story in stories {
        if !story_ids.insert(story.id()) {
            return Err(StoryRegistryError::DuplicateStoryId { id: story.id() });
        }
        if story.variants().is_empty() {
            return Err(StoryRegistryError::StoryWithoutVariants {
                story_id: story.id(),
            });
        }
        if let Some(variant_id) = duplicate_variant_id(story) {
            return Err(StoryRegistryError::DuplicateVariantId {
                story_id: story.id(),
                variant_id,
            });
        }
    }
    Ok(())
}

fn duplicate_variant_id(story: &Story) -> Option<&'static str> {
    story
        .variants()
        .iter()
        .enumerate()
        .find_map(|(index, variant)| {
            story.variants()[..index]
                .iter()
                .any(|registered| registered.id() == variant.id())
                .then_some(variant.id())
        })
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::VNode;

    use super::{
        STORY_COUNT_LINEAR_VALIDATION_MAX, Story, StoryRegistryError, StoryVariant, build_registry,
    };

    fn render_empty() -> dioxus::prelude::Element {
        VNode::empty()
    }

    static FIRST_VARIANT: StoryVariant = StoryVariant {
        id: "default",
        name: "Default",
        description: None,
        render: render_empty,
        source: "",
    };
    static DUPLICATE_VARIANT: StoryVariant = StoryVariant {
        id: "default",
        name: "Duplicate",
        description: None,
        render: render_empty,
        source: "",
    };
    static DUPLICATE_VARIANT_STORY: Story = Story {
        id: "duplicate-variant",
        name: "Duplicate variant",
        description: None,
        preview: None,
        variants: &[&FIRST_VARIANT, &DUPLICATE_VARIANT],
    };
    static FIRST_STORY: Story = Story {
        id: "duplicate-story",
        name: "First",
        description: None,
        preview: None,
        variants: &[&FIRST_VARIANT],
    };
    static SECOND_STORY: Story = Story {
        id: "duplicate-story",
        name: "Second",
        description: None,
        preview: None,
        variants: &[&FIRST_VARIANT],
    };
    static EMPTY_STORY: Story = Story {
        id: "empty-story",
        name: "Empty story",
        description: None,
        preview: None,
        variants: &[],
    };

    #[test]
    fn registry_rejects_duplicate_story_ids() {
        assert_eq!(
            build_registry([&FIRST_STORY, &SECOND_STORY]),
            Err(StoryRegistryError::DuplicateStoryId {
                id: "duplicate-story"
            })
        );
    }

    #[test]
    fn registry_rejects_duplicate_variant_ids() {
        assert_eq!(
            build_registry([&DUPLICATE_VARIANT_STORY]),
            Err(StoryRegistryError::DuplicateVariantId {
                story_id: "duplicate-variant",
                variant_id: "default",
            })
        );
    }

    #[test]
    fn large_registry_preserves_first_error_in_display_order() {
        let registrations = vec![&DUPLICATE_VARIANT_STORY; STORY_COUNT_LINEAR_VALIDATION_MAX + 1];
        assert_eq!(
            build_registry(registrations),
            Err(StoryRegistryError::DuplicateVariantId {
                story_id: "duplicate-variant",
                variant_id: "default",
            })
        );
    }

    #[test]
    fn registry_rejects_stories_without_routed_variants() {
        assert_eq!(
            build_registry([&EMPTY_STORY]),
            Err(StoryRegistryError::StoryWithoutVariants {
                story_id: "empty-story",
            })
        );
    }

    #[test]
    fn catalog_preview_falls_back_to_the_first_routed_variant() {
        assert_eq!(FIRST_STORY.first_variant(), Some(&FIRST_VARIANT));
        assert_eq!(FIRST_STORY.catalog_preview(), Some(&FIRST_VARIANT));
    }
}
