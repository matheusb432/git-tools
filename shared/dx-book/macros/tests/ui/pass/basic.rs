use dioxus::prelude::*;
use storybook::{story, variant};

const EXTRA_DOCUMENTATION: &str = "Extra documentation.";

#[variant]
fn default_state() -> Element {
    rsx! { button { "Default" } }
}

/// A downstream story using a renamed facade dependency.
#[story(
    id = "renamed-dependency",
    name = "Renamed dependency",
    extra_docs = EXTRA_DOCUMENTATION
)]
const RENAMED_DEPENDENCY_STORY: () = &[default_state];

fn main() {
    let _ = storybook::find("renamed-dependency", "default-state");
}
