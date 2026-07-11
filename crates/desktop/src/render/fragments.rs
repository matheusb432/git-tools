use domain::viewer::{
    DiffDensity, DiffLayout, Theme, ViewerDocument, ViewerHistoryEntry, ViewerSettings, ViewerTab,
    ViewerTabId, ViewerTabKind, ViewerTabState, ViewerView,
};
use maud::{Markup, html};

use super::{ViewerRoute, ViewerSettingChange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SwapMode {
    Primary,
    OutOfBand,
}

impl SwapMode {
    const fn out_of_band(self) -> Option<&'static str> {
        match self {
            Self::Primary => None,
            Self::OutOfBand => Some("outerHTML"),
        }
    }
}

pub(super) fn tabs(
    tabs: &[ViewerTab],
    active_tab_id: Option<ViewerTabId>,
    swap: SwapMode,
) -> Markup {
    html! {
        nav id="viewer-tabs" hx-swap-oob=[swap.out_of_band()] class="viewer-tabs" aria-label="Open diffs" {
            ul.viewer-tab-list {
                @for tab in tabs {
                    @let active = Some(tab.id()) == active_tab_id;
                    li class=(if active { "viewer-tab active" } else { "viewer-tab" }) {
                        button type="button"
                            class="viewer-tab-activate"
                            aria-current=[active.then_some("page")]
                            title=(tab.label())
                            hx-get=(ViewerRoute::Activate { tab: tab.id() })
                            hx-target="#viewer-view"
                            hx-swap="outerHTML" {
                            span.viewer-tab-kind aria-hidden="true" { (tab_kind_label(tab.kind())) }
                            span.viewer-tab-label { (tab.label()) }
                            (tab_state_marker(tab.state()))
                        }
                        button type="button"
                            class="viewer-tab-close"
                            aria-label={ "Close " (tab.label()) }
                            title="Close tab"
                            hx-get=(ViewerRoute::Close { tab: tab.id() })
                            hx-target="#viewer-tabs"
                            hx-swap="outerHTML" { "×" }
                    }
                }
            }
            button type="button"
                class="viewer-history-button"
                hx-get=(ViewerRoute::History)
                hx-target="#viewer-history"
                hx-swap="outerHTML"
                popovertarget="viewer-history-popover" {
                "History"
                @if !tabs.is_empty() {
                    span.viewer-count { (tabs.len()) }
                }
            }
        }
    }
}

pub(super) fn view(document: &ViewerDocument, swap: SwapMode) -> Markup {
    html! {
        section id="viewer-view" hx-swap-oob=[swap.out_of_band()] class="viewer-view" data-tab-id=[document.active_tab_id().map(|id| id.to_string())] {
            @match document.active_tab() {
                None => (empty_view()),
                Some(tab) => @match tab.state() {
                    ViewerTabState::Ready => {
                        @let view = document.active_view().expect("ViewerDocument guarantees a view for the ready active tab");
                        (view_controls(view, document.settings()))
                        (infra::html_renderer::build_view_fragment(view.view(), view.options()))
                    },
                    ViewerTabState::Broken { code, reason } => (broken_view(tab.id(), code, reason)),
                    ViewerTabState::Error { reason } => (error_view(tab.id(), reason)),
                }
            }
        }
    }
}

pub(super) fn history(entries: &[ViewerHistoryEntry]) -> Markup {
    html! {
        section id="viewer-history" class="viewer-history" aria-label="Recent diff previews" {
            @if entries.is_empty() {
                div.viewer-history-empty {
                    strong { "No history yet" }
                    p { "Rendered diffs will appear here after you open them." }
                }
            } @else {
                div.viewer-history-columns aria-hidden="true" {
                    span { "Diff" }
                    span { "Repository" }
                    span { "Kind" }
                    span { "Range" }
                    span { "Rendered" }
                }
                div.viewer-history-list {
                    @for entry in entries {
                        button type="button"
                            class="viewer-history-row"
                            hx-get=(ViewerRoute::OpenHistory { render: entry.id() })
                            hx-target="#viewer-tabs"
                            hx-swap="outerHTML" {
                            span.viewer-history-title { (entry.title()) }
                            span.viewer-history-repo { (entry.repo_name()) }
                            span.viewer-history-kind { (entry.kind()) }
                            span.viewer-history-range { (entry.range_label()) }
                            time datetime=(entry.rendered_at()) { (entry.rendered_at()) }
                        }
                    }
                }
            }
        }
    }
}

fn view_controls(view: &ViewerView, settings: &ViewerSettings) -> Markup {
    html! {
        header.viewer-controls aria-label="Diff display controls" {
            div.viewer-control-group role="group" aria-label="Layout" {
                span.viewer-control-label { "Layout" }
                (layout_choice(view, DiffLayout::Unified, "Unified"))
                (layout_choice(view, DiffLayout::Split, "Side by side"))
            }
            div.viewer-control-group role="group" aria-label="Density" {
                span.viewer-control-label { "View" }
                (density_choice(view, DiffDensity::Compact, "Changes"))
                (density_choice(view, DiffDensity::Full, "Full file"))
            }
            @if view.kind() == ViewerTabKind::Live {
                button type="button"
                    class="viewer-control-button"
                    hx-get=(ViewerRoute::Refresh { tab: view.tab_id() })
                    hx-target="#viewer-view"
                    hx-swap="outerHTML" { "Refresh" }
            }
            div.viewer-spacer {}
            div.viewer-control-group role="group" aria-label="Theme" {
                span.viewer-control-label { "Theme" }
                (theme_choice(settings, Theme::Dark, "Dark"))
                (theme_choice(settings, Theme::Light, "Light"))
                (theme_choice(settings, Theme::Hearth, "Hearth"))
            }
        }
    }
}

fn layout_choice(view: &ViewerView, layout: DiffLayout, label: &str) -> Markup {
    let options = view.options().with_layout(layout);
    html! {
        label class="viewer-choice"
            hx-get=(ViewerRoute::View { tab: view.tab_id(), options })
            hx-trigger="change from:find input"
            hx-params="none"
            hx-target="#viewer-view"
            hx-swap="outerHTML" {
            input type="radio" name="viewer-layout" value=(layout)
                checked[view.options().layout() == layout]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Layout(layout)))
                hx-trigger="change"
                hx-params="none"
                hx-swap="none";
            span { (label) }
        }
    }
}

fn density_choice(view: &ViewerView, density: DiffDensity, label: &str) -> Markup {
    let options = view.options().with_density(density);
    html! {
        label class="viewer-choice"
            hx-get=(ViewerRoute::View { tab: view.tab_id(), options })
            hx-trigger="change from:find input"
            hx-params="none"
            hx-target="#viewer-view"
            hx-swap="outerHTML" {
            input type="radio" name="viewer-density" value=(density)
                checked[view.options().density() == density]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Density(density)))
                hx-trigger="change"
                hx-params="none"
                hx-swap="none";
            span { (label) }
        }
    }
}

fn theme_choice(settings: &ViewerSettings, theme: Theme, label: &str) -> Markup {
    html! {
        label class="viewer-choice" {
            input type="radio" name="viewer-theme" value=(theme)
                data-viewer-theme=(theme)
                checked[settings.theme() == theme]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Theme(theme)))
                hx-trigger="change"
                hx-params="none"
                hx-swap="none";
            span { (label) }
        }
    }
}

fn broken_view(tab: ViewerTabId, code: &str, reason: &str) -> Markup {
    html! {
        div.viewer-status.viewer-status-broken role="status" {
            span.viewer-status-mark aria-hidden="true" { "!" }
            div {
                p.viewer-status-eyebrow { "Unavailable · " code { (code) } }
                h1 { "This diff cannot be opened" }
                p { (reason) }
                button type="button"
                    class="viewer-recovery-button"
                    hx-get=(ViewerRoute::Refresh { tab })
                    hx-target="#viewer-view"
                    hx-swap="outerHTML" { "Try again" }
            }
        }
    }
}

fn error_view(tab: ViewerTabId, reason: &str) -> Markup {
    html! {
        div.viewer-status.viewer-status-error role="alert" {
            span.viewer-status-mark aria-hidden="true" { "×" }
            div {
                p.viewer-status-eyebrow { "Render failed" }
                h1 { "The diff could not be rendered" }
                p { (reason) }
                button type="button"
                    class="viewer-recovery-button"
                    hx-get=(ViewerRoute::Refresh { tab })
                    hx-target="#viewer-view"
                    hx-swap="outerHTML" { "Render again" }
            }
        }
    }
}

fn empty_view() -> Markup {
    html! {
        div.viewer-status.viewer-status-empty {
            span.viewer-status-mark aria-hidden="true" { "±" }
            div {
                p.viewer-status-eyebrow { "Viewer ready" }
                h1 { "No diff open" }
                p { "Run " code { "gtl diff" } " in a repository, or choose a previous render from History." }
                button type="button" class="viewer-recovery-button" popovertarget="viewer-history-popover" { "Open history" }
            }
        }
    }
}

fn tab_kind_label(kind: ViewerTabKind) -> &'static str {
    match kind {
        ViewerTabKind::Snapshot => "S",
        ViewerTabKind::Live => "L",
    }
}

fn tab_state_marker(state: &ViewerTabState) -> Markup {
    match state {
        ViewerTabState::Ready => html! {},
        ViewerTabState::Broken { .. } => html! {
            span.viewer-tab-state.broken aria-label="Unavailable" title="Unavailable" { "!" }
        },
        ViewerTabState::Error { .. } => html! {
            span.viewer-tab-state.error aria-label="Render failed" title="Render failed" { "×" }
        },
    }
}
