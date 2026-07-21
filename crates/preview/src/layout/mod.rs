//! The shared `.layout` body: the Shelf 3-column grid (file tree left, diff
//! center, commit shelf right) with titlebar and keybar spanning the full
//! width. One module per region; this module owns their composition.

mod files;
mod keybar;
mod shelf;
mod titlebar;
mod tree;

use application::{diffs::View, viewer::RenderOptions};
use maud::{Markup, html};

const LAYOUT_PRESENTATION_CLASSES: &str = concat!(
    "layout copy-ctx grid h-screen grid-cols-[262px_minmax(0,1fr)_252px] grid-rows-[auto_1fr_auto] ",
    "[@media(min-width:1600px)_and_(min-height:900px)]:grid-cols-[320px_minmax(0,1fr)_304px] ",
    "[@media(min-width:1025px)_and_(max-width:1280px)]:grid-cols-[220px_minmax(0,1fr)_210px] ",
    "[@media(max-width:1024px)]:grid-cols-[0_minmax(0,1fr)_0] ",
    "print:block print:h-auto print:bg-white print:text-[#111]",
);

// Body-only markup (the .layout block) shared by the single and tabbed views so the
// document chrome — one <style>/<script> — lives only at the top level. Per-commit
// [popover] elements live inside .layout so the per-layout JS scoping in preview.js
// finds them.
pub(crate) fn view_body(view: &View, options: RenderOptions) -> Markup {
    html! {
        div class={
            (LAYOUT_PRESENTATION_CLASSES) " "
            (crate::rows::ROW_PRESENTATION_CLASSES) " "
            (crate::rows::SPLIT_PRESENTATION_CLASSES) " "
            (crate::rows::INTRALINE_PRESENTATION_CLASSES)
        } {
            (titlebar::titlebar(view))
            (tree::tree(view))
            main class="main [grid-area:2/2] overflow-auto px-[22px] pt-4 pb-[60px] [@media(min-width:1600px)_and_(min-height:900px)]:px-7 [@media(min-width:1025px)_and_(max-width:1280px)]:px-4 [@media(max-width:1024px)]:px-3 [@media(max-width:1024px)]:pb-12 print:overflow-visible print:p-0" {
                (files::file_blocks(view, options))
            }
            (shelf::shelf(view))
            (keybar::keybar(view))
            (shelf::commit_popovers(view))
        }
    }
}

#[cfg(test)]
mod tests {
    use application::viewer::RenderOptions;

    use crate::{fixtures::sample_view, view_fragment};

    fn layout_classes(html: &str) -> &str {
        html.split_once(r#"<div class="layout "#)
            .and_then(|(_, tail)| tail.split_once('"'))
            .map(|(classes, _)| classes)
            .expect("layout class attribute")
    }

    #[test]
    fn view_fragment_omits_document_chrome_and_presentation_controls() {
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT).into_string();

        assert!(!html.contains(r#"class="layout-toggle""#));
        assert!(!html.contains(r#"class="view-toggle""#));
        assert!(!html.contains(r#"class="theme-select""#));
        assert!(!html.contains("localStorage"));
        assert!(html.contains(r#"<aside class="tree "#));
        assert!(html.contains(r#"<aside class="shelf"#));
        assert!(html.contains(r#"<div id="pop-abc123def" class="print:hidden! "#));
        assert!(html.contains("popover>"));
        assert_eq!(html.matches("single-variant").count(), 1);
        assert_eq!(html.matches(r#"class="diff "#).count(), 1);
    }

    #[test]
    fn build_html_wires_copy_context_toggle_and_per_file_comment_leader() {
        let html = crate::build_html(&sample_view(), RenderOptions::DEFAULT, None);

        // context-on by default: the root carries the class the copy-button reads at click time
        assert!(html.contains(r#"class="layout copy-ctx "#));
        // header toggle starts active/pressed
        assert!(html.contains(r#"class="ctx-toggle active "#));
        assert!(html.contains(r#"aria-pressed="true""#));
        // the .rs file advertises the // comment leader for the pasteable header
        assert!(html.contains(r#"data-comment="//""#));
    }

    #[test]
    fn view_body_carries_grid_scroll_and_print_contracts() {
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT)
            .into_string()
            .replace("&amp;", "&");
        let layout = layout_classes(&html);
        eprintln!("layout_root_class_bytes={}", layout.len());

        assert!(html.contains(r#"class="layout copy-ctx grid"#));
        assert!(html.contains("grid-cols-[262px_minmax(0,1fr)_252px]"));
        assert!(html.contains("[@media(max-width:1024px)]:grid-cols-[0_minmax(0,1fr)_0]"));
        assert!(html.contains("grid-rows-[auto_1fr_auto]"));
        assert!(html.contains("[&_.dl]:grid-cols-[44px_44px_minmax(0,1fr)]"));
        assert!(
            html.contains("[&_.diff-split_.dl]:grid-cols-[44px_minmax(0,1fr)_44px_minmax(0,1fr)]")
        );
        assert!(html.contains("print:block"));
        assert!(html.contains(r#"<aside class="tree "#));
        assert!(html.contains("print:hidden!"));
        assert!(html.contains("print:block!"));

        for static_selector in [
            "[&_.tree]",
            "[&_.shelf]",
            "[&_.titlebar]",
            "[&_.branchline]",
            "[&_.foldall]",
            "[&_.ctx-toggle]",
            "[&_.copy-button]",
            "[&_.empty]",
            "[&_details.file]",
            "[&_.filebody]",
            "[&_.diff::",
            "print:[&_.dl_code]",
        ] {
            assert!(
                !layout.contains(static_selector),
                "static selector `{static_selector}` must live on its Maud owner"
            );
        }
        assert!(
            layout.len() < 4_000,
            "layout root class bytes must stay bounded, got {}",
            layout.len()
        );
        let main = html
            .split_once(r#"<main class=""#)
            .and_then(|(_, tail)| tail.split_once('"'))
            .map(|(classes, _)| classes)
            .expect("main class attribute");
        assert!(
            main.split_ascii_whitespace()
                .any(|class| class == "overflow-auto")
        );
        assert!(!main.contains(concat!("scroll-", "smooth")));
        assert_eq!(html.matches(r"<main class=").count(), 1);
    }
}
