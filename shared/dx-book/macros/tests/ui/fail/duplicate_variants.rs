use dioxus::prelude::*;
use storybook::{story, variant};

#[variant]
fn default() -> Element {
    rsx! { "Default" }
}

#[story(id = "duplicate", name = "Duplicate")]
const DUPLICATE: () = &[default, default];

fn main() {}
