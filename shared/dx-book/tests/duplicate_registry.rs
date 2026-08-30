use dioxus::prelude::VNode;
use dx_book::{Story, StoryRegistryError, StoryVariant, stories};

fn render_empty() -> dioxus::prelude::Element {
    VNode::empty()
}

static VARIANT: StoryVariant =
    dx_book::__private::story_variant("default", "Default", None, render_empty, "");
static FIRST: Story =
    dx_book::__private::story("distributed-duplicate", "First", None, None, &[&VARIANT]);
static SECOND: Story =
    dx_book::__private::story("distributed-duplicate", "Second", None, None, &[&VARIANT]);

dx_book::__private::submit! {
    &FIRST
}
dx_book::__private::submit! {
    &SECOND
}

#[test]
fn distributed_registry_reports_duplicate_story_ids() {
    assert_eq!(
        stories(),
        Err(StoryRegistryError::DuplicateStoryId {
            id: "distributed-duplicate"
        })
    );
}
