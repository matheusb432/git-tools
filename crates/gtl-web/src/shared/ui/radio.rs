use dioxus::prelude::*;

#[component]
pub(crate) fn Radio(
    name: String,
    value: String,
    label: String,
    checked: bool,
    #[props(default)] disabled: bool,
    onchange: EventHandler<()>,
) -> Element {
    rsx! {
        label { class: "control-choice",
            input {
                r#type: "radio",
                class: "control-radio",
                name,
                value,
                checked,
                disabled,
                onchange: move |_| onchange.call(()),
            }
            span { class: "control-choice-label", "{label}" }
        }
    }
}
