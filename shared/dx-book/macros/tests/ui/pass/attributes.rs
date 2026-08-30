use dioxus::prelude::*;
use storybook::{story, variant};

mod public_variant {
    use dioxus::prelude::*;
    use storybook::variant;

    #[variant]
    pub fn visible() -> Element {
        rsx! { "Visible" }
    }
}

#[allow(clippy::let_unit_value)]
#[variant]
fn default() -> Element {
    let _value = ();
    rsx! { "Default" }
}

#[allow(non_upper_case_globals)]
#[story(id = "attributes", name = "Attributes")]
const attributes: () = &[default];

fn main() {
    let _ = public_variant::visible();
}
