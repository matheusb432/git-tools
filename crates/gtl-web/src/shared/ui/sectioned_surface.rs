use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[component]
pub(crate) fn SectionedSurface(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(section {
        class: "control-sectioned-surface",
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        section { ..attributes,{children} }
    }
}

#[component]
pub(crate) fn SectionedSurfaceHeader(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(header {
        class: "control-sectioned-surface-header",
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        header { ..attributes,{children} }
    }
}

#[component]
pub(crate) fn SectionedSurfaceBody(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "control-sectioned-surface-body",
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        div { ..attributes,{children} }
    }
}

#[component]
pub(crate) fn SectionedSurfaceFooter(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(footer {
        class: "control-sectioned-surface-footer",
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        footer { ..attributes,{children} }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::{
        SectionedSurface, SectionedSurfaceBody, SectionedSurfaceFooter, SectionedSurfaceHeader,
    };

    #[test]
    fn surface_parts_keep_the_shared_presentation_contract() {
        let html = dioxus_ssr::render_element(rsx! {
            SectionedSurface { aria_label: "Example surface",
                SectionedSurfaceHeader { "Header" }
                SectionedSurfaceBody { "Body" }
                SectionedSurfaceFooter { "Footer" }
            }
        });

        assert!(html.contains("control-sectioned-surface"));
        assert!(html.contains("control-sectioned-surface-header"));
        assert!(html.contains("control-sectioned-surface-body"));
        assert!(html.contains("control-sectioned-surface-footer"));
        assert!(html.contains(r#"aria-label="Example surface""#));
    }
}
