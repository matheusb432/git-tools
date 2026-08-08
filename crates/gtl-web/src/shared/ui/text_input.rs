use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const TEXT_INPUT_CLASSES: &str = "h-9 w-full rounded-sm border border-line-2 bg-sunk px-3 text-sm text-ink outline-none placeholder:text-ink-3 hover:border-ink-3 focus-visible:border-acc focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc disabled:cursor-not-allowed disabled:bg-surface-2 disabled:text-ink-3 aria-invalid:border-del aria-invalid:focus-visible:outline-del";

#[component]
pub(crate) fn TextInput(
    label: String,
    supporting_content: Option<Element>,
    #[props(extends = GlobalAttributes)]
    #[props(extends = input)]
    attributes: Vec<Attribute>,
    oninput: Option<EventHandler<FormEvent>>,
) -> Element {
    let base = attributes!(input {
        class: TEXT_INPUT_CLASSES,
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        div { class: "grid min-w-0 gap-1.5",
            label { class: "grid min-w-0 gap-1.5",
                span { class: "text-xs font-semibold text-ink", "{label}" }
                input {
                    oninput: move |event| {
                        if let Some(handler) = &oninput {
                            handler.call(event);
                        }
                    },
                    ..attributes,
                }
            }
            if let Some(supporting_content) = supporting_content {
                div { class: "text-xs leading-5 text-ink-2", {supporting_content} }
            }
        }
    }
}
