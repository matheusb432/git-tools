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
        SearchPanelPlacement::DocumentEnd => "control-search-panel-document-end",
        SearchPanelPlacement::WorkspaceCenter => "control-search-panel-workspace-center",
    };
    rsx! {
        section {
            class: "control-search-panel {position}",
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
