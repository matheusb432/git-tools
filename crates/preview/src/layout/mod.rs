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

// Body-only markup (the .layout block) shared by the single and tabbed views so the
// document chrome — one <style>/<script> — lives only at the top level. Per-commit
// [popover] elements live inside .layout so the per-layout JS scoping in preview.js
// finds them.
pub(crate) fn view_body(view: &View, options: RenderOptions) -> Markup {
    html! {
        div.layout.copy-ctx {
            (titlebar::titlebar(view))
            (tree::tree(view))
            main.main {
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

    use crate::{fixtures::sample_view, preview_css, view_fragment};

    #[test]
    fn view_fragment_omits_document_chrome_and_presentation_controls() {
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT).into_string();

        assert!(!html.contains(r#"class="layout-toggle""#));
        assert!(!html.contains(r#"class="view-toggle""#));
        assert!(!html.contains(r#"class="theme-select""#));
        assert!(!html.contains("localStorage"));
        assert!(html.contains(r#"<aside class="tree "#));
        assert!(html.contains(r#"<aside class="shelf"#));
        assert!(html.contains(r#"<div id="pop-abc123def" popover>"#));
        assert!(preview_css().contains(".filebody.single-variant .diff"));
    }

    #[test]
    fn build_html_wires_copy_context_toggle_and_per_file_comment_leader() {
        let html = crate::build_html(&sample_view(), RenderOptions::DEFAULT, None);

        // context-on by default: the root carries the class the copy-button reads at click time
        assert!(html.contains(r#"class="layout copy-ctx""#));
        // header toggle starts active/pressed
        assert!(html.contains(r#"class="ctx-toggle active" aria-pressed="true""#));
        // the .rs file advertises the // comment leader for the pasteable header
        assert!(html.contains(r#"data-comment="//""#));
    }
}
