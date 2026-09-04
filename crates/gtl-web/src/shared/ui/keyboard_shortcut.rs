use dioxus::prelude::*;

#[component]
pub(crate) fn KeyboardShortcut(keys: Vec<&'static str>) -> Element {
    rsx! {
        span { class: "inline-flex items-center gap-1", aria_hidden: "true",
            for key in keys {
                kbd { class: "min-w-5 rounded-sm border border-line-2 border-b-2 bg-sunk px-1 py-px text-center text-[0.6875rem] font-medium leading-4 text-ink-2",
                    "{key}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::KeyboardShortcut;

    #[test]
    fn renders_keys_in_declared_order() {
        const KEYS: [&str; 3] = ["First", "Second", "Third"];
        let html = dioxus_ssr::render_element(rsx! {
            KeyboardShortcut { keys: KEYS.to_vec() }
        });

        assert!(html.contains(">First<"));
        assert!(html.contains(">Second<"));
        assert!(html.contains(">Third<"));
        let first = html.find(">First<").unwrap_or_default();
        let second = html.find(">Second<").unwrap_or_default();
        let third = html.find(">Third<").unwrap_or_default();
        assert!(first < second && second < third);
    }
}
