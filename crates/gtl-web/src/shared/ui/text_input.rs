use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const TEXT_INPUT_CLASSES: &str = "h-9 w-full rounded-sm border border-line-2 bg-sunk px-2.5 font-mono text-[13px] text-ink outline-none placeholder:text-ink-3 hover:border-ink-3 focus-visible:border-acc-line focus-visible:shadow-[0_0_0_2px_var(--color-acc-soft)] disabled:cursor-not-allowed disabled:bg-surface-2 disabled:text-ink-3 aria-invalid:border-del aria-invalid:focus-visible:shadow-[0_0_0_2px_var(--color-del-bg)]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TextInputLabelVisibility {
    #[default]
    Visible,
    Hidden,
}

#[component]
pub(crate) fn TextInput(
    label: String,
    #[props(default)] label_visibility: TextInputLabelVisibility,
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
        div { class: if label_visibility == TextInputLabelVisibility::Hidden { "min-w-0" } else { "grid min-w-0 gap-1.5" },
            label { class: if label_visibility == TextInputLabelVisibility::Hidden { "block min-w-0" } else { "grid min-w-0 gap-1.5" },
                span { class: if label_visibility == TextInputLabelVisibility::Hidden { "sr-only" } else { "text-xs font-semibold text-ink" }, "{label}" }
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
