use dioxus::prelude::*;

#[component]
pub(crate) fn Radio(
    name: String,
    value: String,
    label: String,
    hint: Option<String>,
    checked: bool,
    #[props(default)] disabled: bool,
    onchange: EventHandler<()>,
) -> Element {
    let hint = hint.map(|text| (format!("{name}-{value}-hint"), text));
    let described_by = hint.as_ref().map(|(hint_id, _)| hint_id.clone());
    rsx! {
        label { class: "control-choice",
            input {
                r#type: "radio",
                class: "control-radio",
                name,
                value,
                checked,
                disabled,
                aria_describedby: described_by,
                onchange: move |_| onchange.call(()),
            }
            span {
                span { class: "control-choice-label", "{label}" }
                if let Some((hint_id, hint)) = hint {
                    span { id: hint_id, class: "control-choice-hint", "{hint}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::Radio;

    fn render(app: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn hint_describes_its_radio() {
        let html = render(|| {
            rsx! {
                Radio {
                    name: "export-format",
                    value: "markdown",
                    label: "Markdown",
                    hint: "Locations, excerpts, and comments.",
                    checked: true,
                    onchange: |()| {},
                }
            }
        });

        assert!(html.contains("aria-describedby=\"export-format-markdown-hint\""));
        assert!(html.contains("id=\"export-format-markdown-hint\""));
    }

    #[test]
    fn radio_without_hint_describes_nothing() {
        let html = render(|| {
            rsx! {
                Radio {
                    name: "layout",
                    value: "split",
                    label: "Split",
                    checked: false,
                    onchange: |()| {},
                }
            }
        });

        assert!(!html.contains("aria-describedby"));
        assert!(!html.contains("control-choice-hint"));
    }
}
