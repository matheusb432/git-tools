//! The tab-stripped multi-repo document (diff-subrepos / diff --all): one
//! panel per repo view behind a sticky tab strip.

use application::diffs::View;
use maud::{DOCTYPE, PreEscaped, html};

use super::THEME_BOOT_JS;
use crate::{
    assets::{PREVIEW_BUNDLE, preview_css},
    layout::{Surface, view_body},
};

// ? tab strip CSS stays its own sheet; tab logic is in the bundle (tabbed.ts, guarded to
// no-op without .tabs)
const TABBED_CSS: &str = include_str!("tabbed.css");

pub fn build_tabbed_html(title: &str, views: &[View]) -> String {
    let default_theme = views.first().and_then(|v| v.theme.as_deref());
    html! {
        (DOCTYPE)
        html lang="en" data-theme=[default_theme] {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                // ! page ships its own dark theme + switcher — tell Dark Reader to leave it alone.
                meta name="darkreader-lock";
                title { (title) }
                script { (PreEscaped(THEME_BOOT_JS)) }
                style { (PreEscaped(preview_css())) }
                style { (PreEscaped(TABBED_CSS)) }
            }
            body {
                nav.tabs role="tablist" aria-label="Subrepo diffs" {
                    @for (index, view) in views.iter().enumerate() {
                        button class=(if index == 0 { "tab active" } else { "tab" })
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
                    section.panel id={ "panel-" (index) } role="tabpanel" aria-labelledby={ "tab-" (index) } hidden[index != 0] {
                        (view_body(view, Surface::Artifact))
                    }
                }
                script { (PreEscaped(PREVIEW_BUNDLE)) }
            }
        }
    }
    .into_string()
}
