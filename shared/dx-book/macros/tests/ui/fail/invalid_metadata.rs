use dioxus::prelude::*;
use storybook::{story, variant};

#[variant(id = "bad--id", name = " Padded ")]
fn bad__name() -> Element {
    rsx! { "Bad" }
}

#[story(id = "bad-_story", name = " Story ")]
const BAD: () = &[bad__name];

fn main() {}
