use dioxus::prelude::*;

fn field_error_message_for_transition(
    message_current: Option<String>,
    message_previous: Option<&String>,
) -> String {
    message_current
        .or_else(|| message_previous.cloned())
        .unwrap_or_default()
}

#[component]
pub(crate) fn FieldError(message: Option<String>, id: Option<String>) -> Element {
    let has_error = message.is_some();
    let mut message_previous = use_signal(String::new);
    let message_for_effect = message.clone();
    use_effect(use_reactive(
        (&message_for_effect,),
        move |(message_for_effect,)| {
            if let Some(message) = message_for_effect {
                message_previous.set(message);
            }
        },
    ));
    let message_previous_value = message_previous.peek().clone();
    let message_displayed = field_error_message_for_transition(
        message.clone(),
        (!message_previous_value.is_empty()).then_some(&message_previous_value),
    );

    rsx! {
        p {
            id,
            class: "control-field-error",
            "data-visible": has_error.then_some("true"),
            span { aria_hidden: "true", "{message_displayed}" }
            span { class: "sr-only", role: "alert", aria_atomic: "true",
                {message.unwrap_or_default()}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::field_error_message_for_transition;

    #[test]
    fn transition_retains_the_last_error_after_it_clears() {
        let message = field_error_message_for_transition(Some("Required field".to_owned()), None);

        assert_eq!(
            field_error_message_for_transition(None, Some(&message)),
            "Required field"
        );
    }
}
