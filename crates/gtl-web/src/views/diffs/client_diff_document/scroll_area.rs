use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout};

use crate::shared::{
    i18n::{t, use_language},
    ui::scroll_area::{
        browser::use_scrollbars,
        scrollbar::{ScrollbarPlacement, ScrollbarRails},
    },
};

#[component]
pub(super) fn DiffRowsScrollArea(
    id: String,
    layout: ViewerDiffLayout,
    density: ViewerDiffDensity,
    line_number_digits: u32,
    footer: Option<Element>,
    children: Element,
) -> Element {
    let scroll = use_scrollbars();
    let style = unified_line_number_width_style(layout, line_number_digits);
    rsx! {
        div { class: "diff-file-scroll",
            div {
                id: id.clone(),
                class: "diff-rows",
                style,
                aria_label: t!(
                    use_language(), "diff-rows-label", layout = layout.as_str(), density = density
                    .as_str()
                ),
                "data-layout": layout.as_str(),
                "data-density": density.as_str(),
                onresize: move |_| scroll.measure(),
                onscroll: move |_| scroll.refresh(),
                div {
                    class: "diff-rows-content",
                    "data-gtl-horizontal-size": "",
                    onresize: move |_| scroll.measure(),
                    {children}
                }
                {footer}
            }
            div { class: "diff-scrollbar-sticky",
                ScrollbarRails {
                    controller: scroll,
                    placement: ScrollbarPlacement::FileBottom,
                    target_id: id.clone(),
                }
            }
        }
    }
}

fn unified_line_number_width_style(
    layout: ViewerDiffLayout,
    line_number_digit_width: u32,
) -> Option<String> {
    (layout == ViewerDiffLayout::Unified)
        .then(|| format!("--unified-line-number-width:calc({line_number_digit_width}ch + 8px)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_width_sizes_only_the_unified_line_number_gutter() {
        assert_eq!(
            unified_line_number_width_style(ViewerDiffLayout::Unified, 5),
            Some("--unified-line-number-width:calc(5ch + 8px)".to_owned())
        );
        assert_eq!(
            unified_line_number_width_style(ViewerDiffLayout::Split, 5),
            None
        );
    }
}
