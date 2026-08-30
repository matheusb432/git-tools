mod variants {
    use dioxus::prelude::*;
    use storybook::variant;

    #[variant]
    pub fn visible() -> Element {
        rsx! { "Visible" }
    }
}

fn main() {
    let _ = variants::VARIANT_VISIBLE;
}
