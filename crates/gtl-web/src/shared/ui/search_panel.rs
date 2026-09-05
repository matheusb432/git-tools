use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SearchPanelPlacement {
    #[default]
    DocumentEnd,
    WorkspaceCenter,
}

#[component]
pub(crate) fn SearchPanel(
    label: String,
    #[props(default)] placement: SearchPanelPlacement,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    onkeydown: EventHandler<KeyboardEvent>,
    onfocusout: Option<EventHandler<FocusEvent>>,
    children: Element,
) -> Element {
    let position = match placement {
        SearchPanelPlacement::DocumentEnd => {
            "top-3 right-5 z-20 tablet:right-3 mobile:top-1 mobile:right-1"
        }
        SearchPanelPlacement::WorkspaceCenter => "top-2 left-1/2 z-60 -translate-x-1/2",
    };
    rsx! {
        section {
            class: "absolute {position} w-[min(34rem,calc(100%-1.5rem))] overflow-hidden rounded-panel border border-line-2 bg-surface shadow-floating",
            role: "search",
            aria_label: label,
            onkeydown,
            onfocusout: move |event| {
                if let Some(handler) = onfocusout {
                    handler.call(event);
                }
            },
            ..attributes,
            {children}
        }
    }
}
