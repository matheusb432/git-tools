use domain::viewer::{DiffDensity, ViewerDocument};
use maud::{DOCTYPE, PreEscaped, html};

use super::{fragments, fragments::SwapMode};
use crate::protocol_config;

const THEME_CONTROL_JS: &str = "(function(){document.addEventListener('change',function(event){var target=event.target;if(!(target instanceof HTMLInputElement))return;var theme=target.dataset.viewerTheme;if(theme)document.documentElement.dataset.theme=theme;});})();";
const PENDING_RECIPES_JS: &str = r##"(async function(){var drain=function(){};await window.__TAURI__.event.listen("recipes-pending",function(){drain();});var chain=Promise.resolve();drain=function(){chain=chain.then(function(){return window.htmx.ajax("GET","/pending",{target:"#viewer-tabs",swap:"outerHTML"});}).catch(function(error){console.error("failed to drain pending recipes",error);});return chain;};await drain();})().catch(function(error){console.error("failed to subscribe to pending recipes",error);});"##;

/// Renders the server-authored viewer document and its independently swappable fragments.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MaudViewerRenderer;

#[allow(
    clippy::unused_self,
    reason = "method syntax keeps the renderer adapter replaceable at route call sites"
)]
impl MaudViewerRenderer {
    pub(crate) fn build_document(self, document: &ViewerDocument) -> String {
        let settings = document.settings();
        let options = settings.options();
        let htmx = format!(
            "(function(){{{};window.htmx=htmx;}})();",
            protocol_config::HTMX
        );

        html! {
            (DOCTYPE)
            html lang="en"
                data-theme=(settings.theme())
                data-diff-layout=(options.layout())
                data-diff-full=(if options.density() == DiffDensity::Full { "on" } else { "off" }) {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    meta name="darkreader-lock";
                    base href=(protocol_config::APP_URL);
                    title { "git-tools viewer" }
                    style { (PreEscaped(infra::html_renderer::preview_css())) }
                    script { (PreEscaped(htmx)) }
                    script { (PreEscaped(THEME_CONTROL_JS)) }
                }
                body.viewer-shell {
                    main.viewer-app {
                        (fragments::tabs(document.tabs(), document.active_tab_id(), SwapMode::Primary))
                        (fragments::view(document, SwapMode::Primary))
                    }
                    aside id="viewer-history-popover" class="viewer-history-popover" popover {
                        header.viewer-history-header {
                            div {
                                strong { "Render history" }
                                span { "Recent diff previews" }
                            }
                            button type="button" class="viewer-icon-button" popovertarget="viewer-history-popover" popovertargetaction="hide" aria-label="Close history" title="Close history" { "×" }
                        }
                        (fragments::history(document.history()))
                    }
                    script { (PreEscaped(infra::html_renderer::preview_bundle())) }
                    script { (PreEscaped(PENDING_RECIPES_JS)) }
                }
            }
        }
        .into_string()
    }

    pub(crate) fn build_view(self, document: &ViewerDocument) -> String {
        fragments::view(document, SwapMode::Primary).into_string()
    }

    #[cfg(test)]
    pub(crate) fn build_tabs(
        self,
        tabs: &[domain::viewer::ViewerTab],
        active_tab_id: Option<domain::viewer::ViewerTabId>,
    ) -> String {
        fragments::tabs(tabs, active_tab_id, SwapMode::Primary).into_string()
    }

    pub(crate) fn build_history(self, history: &[domain::viewer::ViewerHistoryEntry]) -> String {
        fragments::history(history).into_string()
    }

    pub(crate) fn build_view_with_tabs(self, document: &ViewerDocument) -> String {
        html! {
            (fragments::view(document, SwapMode::Primary))
            (fragments::tabs(document.tabs(), document.active_tab_id(), SwapMode::OutOfBand))
        }
        .into_string()
    }

    pub(crate) fn build_cached_view_with_tabs(
        self,
        view: &str,
        document: &ViewerDocument,
    ) -> String {
        html! {
            (PreEscaped(view))
            (fragments::tabs(document.tabs(), document.active_tab_id(), SwapMode::OutOfBand))
        }
        .into_string()
    }

    pub(crate) fn build_tabs_with_view(self, document: &ViewerDocument) -> String {
        html! {
            (fragments::tabs(document.tabs(), document.active_tab_id(), SwapMode::Primary))
            (fragments::view(document, SwapMode::OutOfBand))
        }
        .into_string()
    }
}
