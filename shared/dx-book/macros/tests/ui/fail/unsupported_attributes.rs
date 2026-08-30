use dioxus::prelude::*;
use storybook::{story, variant};

#[inline]
#[variant]
fn default() -> Element {
    rsx! { "Default" }
}

#[deprecated]
#[story(id = "unsupported", name = "Unsupported")]
const UNSUPPORTED: () = &[default];

fn main() {}
