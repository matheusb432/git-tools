use dioxus::prelude::*;
use gtl_wire::viewer::push::ViewerPushPreview;

#[component]
pub(crate) fn PushConfirmationDetails(preview: ViewerPushPreview) -> Element {
    rsx! {
        dl { class: "push-confirmation-data",
            dt { "Repository" }
            dd { "{preview.repository}" }
            dt { "Destination" }
            dd { "{preview.destination}" }
            dt { "Commits" }
            dd { "{preview.count}" }
            dt { "Selected SHA" }
            dd {
                code { "{preview.commit}" }
            }
        }
        p { class: "push-confirmation-command-label", "Command" }
        pre { class: "push-confirmation-command",
            code { "{preview.command}" }
        }
    }
}
