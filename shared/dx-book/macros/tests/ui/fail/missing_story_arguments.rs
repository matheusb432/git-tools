use dioxus::prelude::*;
use storybook::{story, variant};

#[variant]
fn default() -> Element {
    rsx! { div { "Default" } }
}

#[story(name = "Missing ID")]
const MISSING_ID: () = &[default];

#[story(id = "missing-name")]
const MISSING_NAME: () = &[default];

fn main() {}
