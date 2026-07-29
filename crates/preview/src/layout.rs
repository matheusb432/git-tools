//! The shared `.layout` body: the Shelf 3-column grid (file tree left, diff
//! center, commit shelf right) with titlebar and keybar spanning the full
//! width. One module per region; this module owns their composition.

mod files;
mod keybar;
mod shelf;
mod titlebar;
mod tree;

use application::{
    diffs::View,
    viewer::{RenderOptions, ViewerTabId},
};
use maud::{Markup, html};

/// The host that consumes one rendered `.layout` body.
///
/// The two hosts share every region, but file bodies diverge: the desktop app's
/// wry/WebKitGTK webview never marks swapped-in `content-visibility:auto`
/// subtrees relevant, leaving them permanently unpainted, so only browser
/// artifacts opt into that offscreen-skip optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Surface {
    /// The desktop viewer's embedded webview.
    App { tab_id: ViewerTabId },
    /// Self-contained offline documents opened in a browser.
    Artifact,
}

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
pub(crate) fn view_body(view: &View, options: RenderOptions, surface: Surface) -> Markup {
    view_body_with_mode(view, options, surface, BodyMode::Complete)
}

pub(crate) fn view_body_shell(
    view: &View,
    options: RenderOptions,
    surface: Surface,
    load_id: u64,
) -> Markup {
    view_body_with_mode(view, options, surface, BodyMode::Shell { load_id })
}

#[derive(Clone, Copy)]
enum BodyMode {
    Complete,
    Shell { load_id: u64 },
}

fn view_body_with_mode(
    view: &View,
    options: RenderOptions,
    surface: Surface,
    mode: BodyMode,
) -> Markup {
    html! {
        div class={
            (LAYOUT_PRESENTATION_CLASSES) " "
            (crate::rows::ROW_PRESENTATION_CLASSES) " "
            (crate::rows::SPLIT_PRESENTATION_CLASSES) " "
            (crate::rows::INTRALINE_PRESENTATION_CLASSES)
        } {
            (titlebar::titlebar(view))
            (tree::tree(view))
            main class="main gtl-scroll [grid-area:2/2] overflow-auto px-[22px] pt-0 pb-[60px] [@media(min-width:1600px)_and_(min-height:900px)]:px-7 [@media(min-width:1025px)_and_(max-width:1280px)]:px-4 [@media(max-width:1024px)]:px-3 [@media(max-width:760px)]:px-1 [@media(max-width:1024px)]:pb-12 print:overflow-visible print:p-0" {
                @match mode {
                    BodyMode::Complete => (files::file_blocks(view, options, surface)),
                    BodyMode::Shell { load_id } => {
                        (files::file_block_shells(view, options, surface))
                        div id="viewer-chunk-loader"
                            hx-get=(format!("/loads/{load_id}/next"))
                            hx-trigger="load delay:16ms"
                            hx-target="this"
                            hx-swap="outerHTML" {}
                    }
                }
            }
            (shelf::shelf(view))
            (keybar::keybar(view))
            (shelf::commit_popovers(view))
            @if let Surface::App { .. } = surface {
                (tree::mobile_popover(view))
                (shelf::mobile_popover(view))
            }
        }
    }
}

pub(crate) fn view_chunks(
    view: &View,
    options: RenderOptions,
) -> std::collections::VecDeque<crate::ViewChunk> {
    files::view_chunks(view, options)
}

#[cfg(test)]
mod tests {
    use application::viewer::{RenderOptions, ViewerTabId};

    use crate::{fixtures::sample_view, view_fragment};

    fn tab_id(raw: u64) -> ViewerTabId {
        ViewerTabId::try_new(raw).expect("positive tab id")
    }

    fn layout_classes(html: &str) -> &str {
        html.split_once(r#"<div class="layout "#)
            .and_then(|(_, tail)| tail.split_once('"'))
            .map(|(classes, _)| classes)
            .expect("layout class attribute")
    }

    #[test]
    fn view_fragment_omits_document_chrome_and_presentation_controls() {
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT, tab_id(1)).into_string();

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
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT, tab_id(1))
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
        assert!(main.split_ascii_whitespace().any(|class| class == "pt-0"));
        assert!(
            main.split_ascii_whitespace()
                .any(|class| class == "[@media(max-width:760px)]:px-1")
        );
        assert!(!main.split_ascii_whitespace().any(|class| class == "pt-4"));
        assert!(!main.contains(concat!("scroll-", "smooth")));
        assert_eq!(html.matches(r"<main class=").count(), 1);
    }
}
