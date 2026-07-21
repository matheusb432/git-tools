use application::viewer::{DiffDensity, ViewerDocument};
use maud::{DOCTYPE, PreEscaped, html};

use super::{
    fragments,
    fragments::{SwapFeedback, SwapMode},
};
use crate::protocol_config;

// ! Authored in frontend/inline/*.ts and shipped verbatim (the build copies the bytes;
// ! inline.test.ts pins source/generated identity and plain-JS syntax).
const THEME_CONTROL_JS: &str = include_str!("../embedded/generated/theme-control.js");
const PENDING_RECIPES_JS: &str = include_str!("../embedded/generated/pending-recipes.js");

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
                    style { (PreEscaped(preview::preview_css())) }
                    meta name="htmx-config" content=r#"{"includeIndicatorStyles":false,"scrollBehavior":"instant","globalViewTransitions":false}"#;
                    script { (PreEscaped(htmx)) }
                    script { (PreEscaped(THEME_CONTROL_JS)) }
                }
                body class="viewer-shell overflow-hidden" {
                    main class="grid h-screen min-w-0 grid-rows-[auto_minmax(0,1fr)] bg-bg" {
                        (fragments::tabs(document.tabs(), document.active_tab_id(), SwapMode::Primary, SwapFeedback::None))
                        (fragments::view(document, SwapMode::Primary, SwapFeedback::None))
                    }
                    aside id="viewer-history-popover" class="viewer-history-popover m-auto h-[min(680px,calc(100vh_-_84px))] w-[min(1040px,calc(100vw_-_48px))] max-w-none border-line-2 bg-surface p-0 inset-[42px] shadow-[0_24px_80px_rgba(0,0,0,.72)] [&::backdrop]:bg-[rgba(0,0,0,.42)] [@media(max-width:760px)]:h-[calc(100vh_-_24px)] [@media(max-width:760px)]:w-[calc(100vw_-_24px)] [@media(max-width:760px)]:inset-3" popover {
                        header class="viewer-history-header flex items-center justify-between border-b border-line bg-surface-2 px-4 py-[13px]" {
                            div {
                                strong class="block text-[13px] text-ink" { "Render history" }
                                span class="block text-[11px] text-ink-3" { "Recent diff previews" }
                            }
                            button type="button" class="viewer-icon-button size-[30px] cursor-pointer rounded-sm border-0 bg-transparent text-xl text-ink-2 [font:inherit] hover:bg-line hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc" popovertarget="viewer-history-popover" popovertargetaction="hide" aria-label="Close history" title="Close history" { "×" }
                        }
                        (fragments::history(document.history()))
                    }
                    script { (PreEscaped(preview::preview_bundle())) }
                    script { (PreEscaped(PENDING_RECIPES_JS)) }
                }
            }
        }
        .into_string()
    }

    pub(crate) fn build_view(self, document: &ViewerDocument) -> String {
        fragments::view(document, SwapMode::Primary, SwapFeedback::None).into_string()
    }

    #[cfg(test)]
    pub(crate) fn build_tabs(
        self,
        tabs: &[application::viewer::ViewerTab],
        active_tab_id: Option<application::viewer::ViewerTabId>,
    ) -> String {
        fragments::tabs(tabs, active_tab_id, SwapMode::Primary, SwapFeedback::None).into_string()
    }

    pub(crate) fn build_history(
        self,
        history: &[application::viewer::ViewerHistoryEntry],
    ) -> String {
        fragments::history(history).into_string()
    }

    pub(crate) fn build_view_with_tabs(self, document: &ViewerDocument) -> String {
        html! {
            (fragments::view(document, SwapMode::Primary, SwapFeedback::None))
            (fragments::tabs(document.tabs(), document.active_tab_id(), SwapMode::OutOfBand, SwapFeedback::None))
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
            (fragments::tabs(document.tabs(), document.active_tab_id(), SwapMode::OutOfBand, SwapFeedback::None))
        }
        .into_string()
    }

    pub(crate) fn build_tabs_with_view(self, document: &ViewerDocument) -> String {
        html! {
            (fragments::tabs(document.tabs(), document.active_tab_id(), SwapMode::Primary, SwapFeedback::None))
            (fragments::view(document, SwapMode::OutOfBand, SwapFeedback::None))
        }
        .into_string()
    }

    pub(crate) fn build_tabs_with_view_after_snapshot_skips(
        self,
        document: &ViewerDocument,
        labels: &[String],
    ) -> String {
        html! {
            (fragments::tabs(document.tabs(), document.active_tab_id(), SwapMode::Primary, SwapFeedback::SnapshotRecipesSkipped(labels)))
            (fragments::view(document, SwapMode::OutOfBand, SwapFeedback::None))
        }
        .into_string()
    }

    pub(crate) fn build_tabs_with_view_after_live_delete(
        self,
        document: &ViewerDocument,
    ) -> String {
        html! {
            (fragments::tabs(document.tabs(), document.active_tab_id(), SwapMode::Primary, SwapFeedback::LiveViewDeleted))
            (fragments::view(document, SwapMode::OutOfBand, SwapFeedback::LiveViewDeleted))
        }
        .into_string()
    }
}
