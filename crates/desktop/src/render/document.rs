use application::viewer::{DiffDensity, ViewerDocument};
use maud::{DOCTYPE, Markup, PreEscaped, html};

use super::{
    fragments,
    fragments::{SwapFeedback, SwapMode, theme},
};
use crate::{materialization::ViewLoadId, protocol_config};

// ! Built by `deno task build` from frontend/viewer/ (Vite lib IIFE); the drift gate
// ! pins the output to its sources.
const VIEWER_JS: &str = include_str!("../embedded/generated/viewer.js");

/// Renders the server-authored viewer document and its independently swappable fragments.
#[derive(Debug, Clone, Copy, Default)]
pub struct MaudViewerRenderer;

#[allow(
    clippy::unused_self,
    reason = "method syntax keeps the renderer adapter replaceable at route call sites"
)]
impl MaudViewerRenderer {
    #[cfg(test)]
    pub(crate) fn build_document(self, document: &ViewerDocument) -> String {
        self.render_document(document, false)
    }

    pub(crate) fn build_deferred_document(self, document: &ViewerDocument) -> String {
        self.render_document(document, true)
    }

    fn render_document(self, document: &ViewerDocument, defer_ready: bool) -> String {
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
                }
                body class="viewer-shell overflow-hidden" {
                    main class="grid h-screen min-w-0 grid-rows-[auto_minmax(0,1fr)] bg-bg" {
                        (tabs(document, SwapMode::Primary, SwapFeedback::None))
                        (fragments::view(document, SwapMode::Primary, SwapFeedback::None, None, defer_ready))
                    }
                    (fragments::loading_template())
                    aside id="viewer-history-popover" class="viewer-history-popover m-auto h-[min(680px,calc(100vh_-_84px))] w-[min(1040px,calc(100vw_-_48px))] max-w-none overflow-hidden border-line-2 bg-surface p-0 inset-[42px] shadow-[0_24px_80px_rgba(0,0,0,.72)] [&::backdrop]:bg-[rgba(0,0,0,.42)] mobile:h-[calc(100vh_-_24px)] mobile:w-[calc(100vw_-_24px)] mobile:inset-3" popover {
                        header class="viewer-history-header flex items-center justify-between border-b border-line bg-surface-2 px-4 py-[13px]" {
                            div {
                                strong class="block text-[13px] text-ink" { "Render history" }
                                span class="block text-[11px] text-ink-3" { "Recent diff previews" }
                            }
                            button type="button" class="viewer-icon-button size-[30px] cursor-pointer rounded-sm border-0 bg-transparent text-xl text-ink-2 [font:inherit] hover:bg-line hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc" popovertarget="viewer-history-popover" popovertargetaction="hide" aria-label="Close history" title="Close history" { "×" }
                        }
                        (fragments::history(document.history()))
                    }
                    (theme::popover(settings.theme()))
                    script { (PreEscaped(preview::preview_bundle())) }
                    script { (PreEscaped(VIEWER_JS)) }
                }
            }
        }
        .into_string()
    }

    #[cfg(any(test, feature = "benchmark-support"))]
    pub fn build_view(self, document: &ViewerDocument) -> String {
        fragments::view(document, SwapMode::Primary, SwapFeedback::None, None, false).into_string()
    }

    #[cfg(test)]
    pub(crate) fn build_tabs(
        self,
        tabs: &[application::viewer::ViewerTab],
        active_tab_id: Option<application::viewer::ViewerTabId>,
        active_theme: application::viewer::Theme,
    ) -> String {
        fragments::tabs(
            tabs,
            active_tab_id,
            active_theme,
            None,
            SwapMode::Primary,
            SwapFeedback::None,
        )
        .into_string()
    }

    pub(crate) fn build_tabs_only(
        self,
        document: &ViewerDocument,
        feedback: SwapFeedback<'_>,
    ) -> String {
        tabs(document, SwapMode::Primary, feedback).into_string()
    }

    pub(crate) fn build_history(
        self,
        history: &[application::viewer::ViewerHistoryEntry],
    ) -> String {
        fragments::history(history).into_string()
    }

    pub(crate) fn build_view_with_tabs(self, document: &ViewerDocument) -> String {
        self.render_view_with_tabs(document, None)
    }

    pub(crate) fn build_materialized_view_with_tabs(
        self,
        document: &ViewerDocument,
        load_id: ViewLoadId,
    ) -> String {
        self.render_view_with_tabs(document, Some(load_id))
    }

    fn render_view_with_tabs(
        self,
        document: &ViewerDocument,
        load_id: Option<ViewLoadId>,
    ) -> String {
        html! {
            (fragments::view(document, SwapMode::Primary, SwapFeedback::None, load_id, false))
            (tabs(document, SwapMode::OutOfBand, SwapFeedback::None))
        }
        .into_string()
    }

    pub(crate) fn build_tabs_with_view(
        self,
        document: &ViewerDocument,
        feedback: SwapFeedback<'_>,
    ) -> String {
        self.render_tabs_with_view(document, feedback, None)
    }

    pub(crate) fn build_materialized_tabs_with_view(
        self,
        document: &ViewerDocument,
        feedback: SwapFeedback<'_>,
        load_id: ViewLoadId,
    ) -> String {
        self.render_tabs_with_view(document, feedback, Some(load_id))
    }

    fn render_tabs_with_view(
        self,
        document: &ViewerDocument,
        feedback: SwapFeedback<'_>,
        load_id: Option<ViewLoadId>,
    ) -> String {
        let view_feedback = match feedback {
            SwapFeedback::LiveViewDeleted if load_id.is_none() => SwapFeedback::LiveViewDeleted,
            SwapFeedback::None
            | SwapFeedback::LiveViewDeleted
            | SwapFeedback::SnapshotRecipesSkipped(_) => SwapFeedback::None,
        };
        html! {
            (tabs(document, SwapMode::Primary, feedback))
            (fragments::view(document, SwapMode::OutOfBand, view_feedback, load_id, false))
        }
        .into_string()
    }
}

fn tabs(document: &ViewerDocument, swap: SwapMode, feedback: SwapFeedback<'_>) -> Markup {
    let mobile_counts = document
        .active_view()
        .map(|view| fragments::MobileNavigationCounts {
            files: view.view().files.len(),
            commits: view.range_view().commits.len(),
        });
    fragments::tabs(
        document.tabs(),
        document.active_tab_id(),
        document.settings().theme(),
        mobile_counts,
        swap,
        feedback,
    )
}
