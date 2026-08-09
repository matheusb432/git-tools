//! The raw artifact's `.layout` body: the Shelf 3-column grid with titlebar
//! and keybar spanning the full width.

mod artifact_controls;
mod diff_document;
mod file_status;
mod files;
mod keybar;
mod shelf;
mod titlebar;
mod tree;

pub(crate) use diff_document::diff_document_shell;
use gtl_application::{diffs::View, viewer::RenderOptions};
use maud::{Markup, html};

use crate::syntax::PreviewResult;

struct ArtifactMobileNavigationTargets {
    files: String,
    commits: String,
    controls: String,
}

impl ArtifactMobileNavigationTargets {
    fn new(view_index: usize) -> Self {
        Self {
            files: format!("preview-files-popover-{view_index}"),
            commits: format!("preview-commits-popover-{view_index}"),
            controls: format!("preview-controls-popover-{view_index}"),
        }
    }
}

const LAYOUT_PRESENTATION_CLASSES: &str = concat!(
    "layout copy-ctx grid h-screen grid-cols-[262px_minmax(0,1fr)_252px] grid-rows-[auto_1fr_auto] ",
    "wide-screen:grid-cols-[320px_minmax(0,1fr)_304px] ",
    "compact-desktop:grid-cols-[220px_minmax(0,1fr)_210px] ",
    "tablet:grid-cols-[0_minmax(0,1fr)_0] ",
    "print:block print:h-auto print:bg-white print:text-[#111]",
);

// Body-only markup shared by the single and tabbed artifact documents so the
// document chrome lives once at the top level. Per-commit popovers stay inside
// `.layout` for the raw enhancer's per-layout scoping.
pub(crate) fn view_body(
    view: &View,
    options: RenderOptions,
    view_index: usize,
) -> PreviewResult<Markup> {
    let mobile_navigation = ArtifactMobileNavigationTargets::new(view_index);
    let changed_files = tree::ChangedFilesPresentation::new(view);
    let commit_shelf = shelf::CommitShelfPresentation::new(view);
    Ok(html! {
        div class={
            (LAYOUT_PRESENTATION_CLASSES) " "
            (crate::rows::ROW_PRESENTATION_CLASSES) " "
            (crate::rows::SPLIT_PRESENTATION_CLASSES) " "
            (crate::rows::INTRALINE_PRESENTATION_CLASSES)
        } {
            (titlebar::titlebar(view, &mobile_navigation))
            (tree::tree(&changed_files))
            main class="main gtl-scroll [grid-area:2/2] overflow-auto px-[22px] pt-0 pb-[60px] wide-screen:px-7 compact-desktop:px-4 tablet:px-3 mobile:px-1 tablet:pb-12 print:overflow-visible print:p-0" {
                (files::file_blocks(view, options)?)
            }
            (shelf::shelf(&commit_shelf))
            (keybar::keybar(view))
            (shelf::commit_popovers(&commit_shelf))
            (tree::mobile_popover(&changed_files, &mobile_navigation.files))
            (shelf::mobile_popover(&commit_shelf, &mobile_navigation.commits))
            (artifact_controls::popover(&mobile_navigation.controls))
        }
    })
}

pub(crate) fn view_chunks(
    view: &View,
    options: RenderOptions,
) -> PreviewResult<std::collections::VecDeque<crate::ViewChunk>> {
    files::view_chunks(view, options)
}

#[cfg(test)]
mod tests {
    use gtl_application::viewer::RenderOptions;

    use crate::{fixtures::sample_view, test_render::build_html};

    fn layout_classes(html: &str) -> &str {
        html.split_once(r#"<div class="layout "#)
            .and_then(|(_, tail)| tail.split_once('"'))
            .map(|(classes, _)| classes)
            .expect("layout class attribute")
    }

    #[test]
    fn raw_layout_omits_presentation_controls() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        assert!(!html.contains(r#"class="layout-toggle""#));
        assert!(!html.contains(r#"class="view-toggle""#));
        assert!(!html.contains(r#"class="theme-select""#));
        assert!(html.contains(r#"<aside class="tree "#));
        assert!(html.contains(r#"<aside class="shelf"#));
        assert!(html.contains(r#"<div id="pop-abc123def" class="print:hidden! "#));
        assert!(html.contains("popover>"));
        assert_eq!(html.matches("single-variant").count(), 1);
        assert_eq!(html.matches(r#"class="diff "#).count(), 1);
    }

    #[test]
    fn build_html_wires_copy_context_toggle_and_per_file_comment_leader() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

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
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None).replace("&amp;", "&");
        let layout = layout_classes(&html);
        eprintln!("layout_root_class_bytes={}", layout.len());

        assert!(html.contains(r#"class="layout copy-ctx grid"#));
        assert!(html.contains("grid-cols-[262px_minmax(0,1fr)_252px]"));
        assert!(html.contains("tablet:grid-cols-[0_minmax(0,1fr)_0]"));
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
                .any(|class| class == "mobile:px-1")
        );
        assert!(!main.split_ascii_whitespace().any(|class| class == "pt-4"));
        assert!(!main.contains(concat!("scroll-", "smooth")));
        assert_eq!(html.matches(r"<main class=").count(), 1);
    }
}
