use dioxus::prelude::*;
use dx_book::{StoryVariant, find, stories, story, variant};

const EXTRA_DOCUMENTATION: &str = "Additional usage guidance.";

/// Default component state.
#[variant]
fn default_state() -> Element {
    rsx! { button { "Default" } }
}

/// Compact component state.
///
///     let compact = true;
#[variant]
fn compact() -> Element {
    rsx! { button { "Compact" } }
}

#[variant(id = "catalog_preview", name = "Catalog preview")]
fn preview() -> Element {
    rsx! { button { "Preview" } }
}

#[variant(name = "Direct hook")]
fn direct_hook() -> Element {
    let count = use_signal(|| 0_u32);
    rsx! { output { "{count}" } }
}

/// Registry test component.
#[story(
    id = "registry-test",
    name = "Registry test",
    extra_docs = EXTRA_DOCUMENTATION,
    preview = preview
)]
const REGISTRY_TEST_STORY: () = &[default_state, compact, direct_hook];

#[test]
fn registered_stories_are_sorted_and_searchable() {
    let registered_stories = stories().unwrap();
    let story_names = registered_stories
        .windows(2)
        .all(|pair| pair[0].name() <= pair[1].name());
    assert!(story_names);

    let registered_story = find("registry-test", "default-state").unwrap();
    assert!(registered_story.is_some());
    if let Some((story, variant)) = registered_story {
        assert_eq!(story.name(), "Registry test");
        assert_eq!(
            story.description(),
            Some("Registry test component.\nAdditional usage guidance.")
        );
        assert_eq!(variant.name(), "Default State");
        assert_eq!(variant.description(), Some("Default component state."));
        assert!(variant.source().contains("fn default_state"));
        assert!(variant.source().ends_with("\n}\n"));
        assert_eq!(
            story.preview().map(StoryVariant::id),
            Some("catalog_preview")
        );
        assert_eq!(
            story.catalog_preview().map(StoryVariant::name),
            Some("Catalog preview")
        );
    }

    let (_, compact_variant) = find("registry-test", "compact").unwrap().unwrap();
    assert_eq!(
        compact_variant.description(),
        Some("Compact component state.\n\n    let compact = true;")
    );
}

#[test]
fn unknown_story_paths_do_not_resolve() {
    assert!(find("registry-test", "missing").unwrap().is_none());
    assert!(find("missing", "default-state").unwrap().is_none());
}
