use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const PAGE_NOTICE_CLASSES: &str = "grid place-content-center text-center";

#[component]
pub(crate) fn PageNotice(
    title: String,
    message: String,
    icon: Option<Element>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let title_classes = if icon.is_some() {
        "mt-3 font-semibold text-ink"
    } else {
        "font-semibold text-ink"
    };
    let base = attributes!(section {
        class: PAGE_NOTICE_CLASSES,
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        section {..attributes,
            if let Some(icon) = icon {
                span { class: "mx-auto text-acc", aria_hidden: "true", {icon} }
            }
            h2 { class: title_classes, "{title}" }
            p { class: "mt-1 max-w-md leading-5 text-ink-2", "{message}" }
            {children}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_preserves_semantics_and_action_content() {
        let html = dioxus_ssr::render_element(rsx! {
            PageNotice {
                class: "min-h-64",
                role: "alert",
                title: "Viewer unavailable",
                message: "Reconnect to continue.",
                button { "Try again" }
            }
        });

        assert!(html.contains(r#"role="alert""#));
        assert!(html.contains("grid place-content-center text-center"));
        assert!(html.contains("min-h-64"));
        assert!(html.contains("<h2"));
        assert!(html.contains("Viewer unavailable"));
        assert!(html.contains("Reconnect to continue."));
        assert!(html.contains("<button>Try again</button>"));
        assert!(!html.contains(r#"aria-hidden="true""#));
    }

    #[test]
    fn notice_icon_is_decorative_and_offsets_the_title() {
        let html = dioxus_ssr::render_element(rsx! {
            PageNotice {
                title: "No diff is open",
                message: "Open a diff to begin.",
                icon: rsx! {
                    svg {}
                },
            }
        });

        assert!(html.contains(r#"aria-hidden="true""#));
        assert!(html.contains("mt-3 font-semibold text-ink"));
    }
}
