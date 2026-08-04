//! The tab-stripped multi-repo document (diff-subrepos / diff --all): one
//! panel per repo view behind a sticky tab strip.

use gtl_application::{diffs::View, viewer::RenderOptions};
use maud::{DOCTYPE, PreEscaped, html};

use super::THEME_BOOT_JS;
use crate::{
    assets::{PREVIEW_BUNDLE, preview_css},
    layout::{Surface, view_body},
    syntax::PreviewResult,
};

const TABS_CLASSES: &str = "gtl-scroll-rail sticky top-0 z-60 flex items-center gap-1.5 overflow-x-auto border-b border-line bg-surface-2 px-3 py-2.5";
const TAB_CLASSES: &str = concat!(
    "max-w-[280px] flex-none cursor-pointer overflow-hidden text-ellipsis whitespace-nowrap rounded-panel border border-line bg-surface px-2.5 py-1.5 text-ink-2 [font:inherit] ",
    "hover:border-acc-line hover:text-ink [&.active]:border-acc-line [&.active]:text-acc",
);
const PANEL_CLASSES: &str = "[&[hidden]]:hidden";

/// Builds a self-contained tabbed HTML document for multiple diff views.
///
/// # Errors
///
/// Returns an error when the embedded syntax-highlighting assets cannot be loaded.
pub fn build_tabbed_html(
    title: &str,
    views: &[View],
    options: RenderOptions,
    theme: Option<&str>,
) -> PreviewResult<String> {
    Ok(html! {
        (DOCTYPE)
        html lang="en" data-theme=[theme] {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                // ! The page ships its own palette, so Dark Reader must leave it alone.
                meta name="darkreader-lock";
                title { (title) }
                script { (PreEscaped(THEME_BOOT_JS)) }
                style { (PreEscaped(preview_css())) }
            }
            body {
                nav class={ "tabs " (TABS_CLASSES) } role="tablist" aria-label="Subrepo diffs" {
                    @for (index, view) in views.iter().enumerate() {
                        button class={ (if index == 0 { "tab active " } else { "tab " }) (TAB_CLASSES) }
                            id={ "tab-" (index) }
                            role="tab"
                            aria-selected=(if index == 0 { "true" } else { "false" })
                            aria-controls={ "panel-" (index) }
                            data-tab=(index) {
                            (view.repo_name)
                        }
                    }
                }
                @for (index, view) in views.iter().enumerate() {
                    section class={ "panel " (PANEL_CLASSES) } id={ "panel-" (index) } role="tabpanel" aria-labelledby={ "tab-" (index) } hidden[index != 0] {
                        (view_body(view, options, Surface::Artifact { view_index: index })?)
                    }
                }
                script { (PreEscaped(PREVIEW_BUNDLE)) }
            }
        }
    }
    .into_string())
}

#[cfg(test)]
mod tests {
    use gtl_application::viewer::RenderOptions;

    use crate::{fixtures::sample_view, test_render::build_tabbed_html};

    #[test]
    fn tabbed_document_keeps_semantic_tab_hooks_in_one_shared_stylesheet() {
        let html = build_tabbed_html(
            "subrepo diff",
            &[sample_view(), sample_view()],
            RenderOptions::DEFAULT,
            None,
        );

        assert_eq!(html.matches("<style>").count(), 1);
        assert!(html.contains(r#"<nav class="tabs "#));
        assert!(html.contains(r#"role="tablist" aria-label="Subrepo diffs""#));
        assert!(html.contains(
            r#"id="tab-0" role="tab" aria-selected="true" aria-controls="panel-0" data-tab="0""#
        ));
        assert!(html.contains(r#"<section class="panel "#));
        assert!(html.contains(r#"id="panel-0" role="tabpanel" aria-labelledby="tab-0""#));
        assert!(html.contains(r#"id="panel-1" role="tabpanel" aria-labelledby="tab-1" hidden>"#));
    }
}
