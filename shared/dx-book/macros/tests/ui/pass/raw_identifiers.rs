use dioxus::prelude::*;
use storybook::{story, variant};

#[variant(id = "type_state", name = "Type state")]
fn r#type() -> Element {
    rsx! { "Type" }
}

#[story(id = "raw_story", name = "Raw story")]
const r#match: () = &[r#type];

fn main() {}
