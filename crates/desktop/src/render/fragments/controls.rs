//! The display controls header: the layout and density radios scoped to the
//! active view, plus the live-view actions.

use application::viewer::{DiffDensity, DiffLayout, Theme, ViewerTabId, ViewerTabKind, ViewerView};
use maud::{Markup, html};

use crate::render::{ViewerRoute, ViewerSettingChange};

pub(super) fn view_controls(view: &ViewerView) -> Markup {
    html! {
        header class="viewer-controls gtl-scroll flex min-w-0 items-center gap-3 border-b border-line bg-surface px-3 py-[7px] text-ink-2 [@media(max-width:760px)]:hidden" aria-label="Diff display controls" {
            div class="flex items-center gap-[3px]" role="group" aria-label="Layout" {
                span class="viewer-control-label mr-[3px] text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase" { "Layout" }
                (layout_choice(view, DiffLayout::Unified, "Unified", "viewer-layout", CHOICE_CLASSES))
                (layout_choice(view, DiffLayout::Split, "Side by side", "viewer-layout", CHOICE_CLASSES))
            }
            div class="flex items-center gap-[3px]" role="group" aria-label="Density" {
                span class="viewer-control-label mr-[3px] text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase" { "View" }
                (density_choice(view, DiffDensity::Compact, "Changes", "viewer-density", CHOICE_CLASSES))
                (density_choice(view, DiffDensity::Full, "Full file", "viewer-density", CHOICE_CLASSES))
            }
            @if view.selected_commit_sha().is_some() {
                button type="button"
                    class="viewer-control-button inline-flex min-h-[27px] cursor-pointer items-center whitespace-nowrap rounded-sm border border-acc-line bg-acc-soft px-2 py-1 text-[11.5px] text-acc [font:inherit] hover:bg-acc hover:text-bg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc"
                    hx-get=(ViewerRoute::View { tab: view.tab_id(), options: view.options() })
                    hx-target="#viewer-view"
                    hx-sync="#viewer-view:replace"
                    hx-swap="outerHTML" { "Show all changes" }
            }
            @if view.kind() == ViewerTabKind::Live {
                (refresh_button(view.tab_id()))
                (delete_live_view_button(view.tab_id()))
            }
            @if view.selected_commit_sha().is_some() {
                button type="button"
                    class=(preview::mobile_menu_button_classes())
                    hx-get=(ViewerRoute::View { tab: view.tab_id(), options: view.options() })
                    hx-target="#viewer-view"
                    hx-sync="#viewer-view:replace"
                    hx-swap="outerHTML" { "Show all changes" }
            }
        }
    }
}

pub(super) fn mobile_view_controls(
    view: &ViewerView,
    active_theme: Theme,
) -> preview::MobileViewControls {
    let display = html! {
        div class="grid gap-3" {
            fieldset class="m-0 border-0 p-0" {
                legend class="mb-1.5 text-[11px] font-medium text-ink-2" { "Layout" }
                div class="grid grid-cols-2 gap-2" {
                    (layout_choice(view, DiffLayout::Unified, "Unified", "viewer-layout-mobile", MOBILE_CHOICE_CLASSES))
                    (layout_choice(view, DiffLayout::Split, "Side by side", "viewer-layout-mobile", MOBILE_CHOICE_CLASSES))
                }
            }
            fieldset class="m-0 border-0 p-0" {
                legend class="mb-1.5 text-[11px] font-medium text-ink-2" { "Density" }
                div class="grid grid-cols-2 gap-2" {
                    (density_choice(view, DiffDensity::Compact, "Changes", "viewer-density-mobile", MOBILE_CHOICE_CLASSES))
                    (density_choice(view, DiffDensity::Full, "Full file", "viewer-density-mobile", MOBILE_CHOICE_CLASSES))
                }
            }
        }
    };
    let viewer_actions = html! {
        div class="grid gap-2" {
            button type="button"
                class=(preview::mobile_menu_button_classes())
                hx-get=(ViewerRoute::History)
                hx-target="#viewer-history"
                hx-swap="outerHTML"
                popovertarget="viewer-history-popover" { "Render history" }
            button type="button"
                class=(preview::mobile_menu_button_classes())
                popovertarget="viewer-theme-popover" {
                "Theme: " (super::theme::label(active_theme))
            }
            @if view.kind() == ViewerTabKind::Live {
                (mobile_refresh_button(view.tab_id()))
                (mobile_delete_live_view_button(view.tab_id()))
            }
        }
    };
    preview::MobileViewControls::viewer(display, viewer_actions)
}

const CHOICE_CLASSES: &str = "inline-flex min-h-[27px] items-center whitespace-nowrap rounded-sm border border-transparent px-2 py-1 text-[11.5px] group-hover:border-line-2 group-hover:bg-surface-2 group-hover:text-ink peer-checked:border-acc-line peer-checked:bg-acc-soft peer-checked:text-acc peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-acc";
const MOBILE_CHOICE_CLASSES: &str = "flex min-h-10 w-full items-center justify-center rounded-sm border border-line-2 bg-surface-2 px-2 py-2 text-center text-[12px] text-ink-2 group-hover:border-acc-line group-hover:text-ink peer-checked:border-acc-line peer-checked:bg-acc-soft peer-checked:text-acc peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-acc";

fn layout_choice(
    view: &ViewerView,
    layout: DiffLayout,
    label: &str,
    input_name: &str,
    classes: &str,
) -> Markup {
    let options = view.options().with_layout(layout);
    html! {
        label class="viewer-choice group relative cursor-pointer"
            hx-get=(view_route(view, options))
            hx-trigger="change from:find input"
            hx-params="none"
            hx-target="#viewer-view"
            hx-sync="#viewer-view:replace"
            hx-swap="outerHTML" {
            input type="radio" class="peer pointer-events-none absolute size-px opacity-0" name=(input_name) value=(layout)
                checked[view.options().layout() == layout]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Layout(layout)))
                hx-trigger="change"
                hx-params="none"
                hx-sync="this:drop"
                hx-swap="none";
            span class=(classes) { (label) }
        }
    }
}

fn density_choice(
    view: &ViewerView,
    density: DiffDensity,
    label: &str,
    input_name: &str,
    classes: &str,
) -> Markup {
    let options = view.options().with_density(density);
    html! {
        label class="viewer-choice group relative cursor-pointer"
            hx-get=(view_route(view, options))
            hx-trigger="change from:find input"
            hx-params="none"
            hx-target="#viewer-view"
            hx-sync="#viewer-view:replace"
            hx-swap="outerHTML" {
            input type="radio" class="peer pointer-events-none absolute size-px opacity-0" name=(input_name) value=(density)
                checked[view.options().density() == density]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Density(density)))
                hx-trigger="change"
                hx-params="none"
                hx-sync="this:drop"
                hx-swap="none";
            span class=(classes) { (label) }
        }
    }
}

fn view_route(view: &ViewerView, options: application::viewer::RenderOptions) -> ViewerRoute {
    view.selected_commit_sha().map_or(
        ViewerRoute::View {
            tab: view.tab_id(),
            options,
        },
        |sha| ViewerRoute::CommitPatch {
            tab: view.tab_id(),
            sha: sha.to_string(),
            options,
        },
    )
}

fn refresh_button(tab: ViewerTabId) -> Markup {
    refresh_button_with_classes(
        tab,
        "viewer-control-button inline-flex min-h-[27px] cursor-pointer items-center whitespace-nowrap rounded-sm border border-transparent bg-transparent px-2 py-1 text-[11.5px] text-inherit [font:inherit] hover:border-line-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-acc-line [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc",
    )
}

fn mobile_refresh_button(tab: ViewerTabId) -> Markup {
    refresh_button_with_classes(tab, preview::mobile_menu_button_classes())
}

fn refresh_button_with_classes(tab: ViewerTabId, classes: &str) -> Markup {
    html! {
        button type="button" class=(classes)
            hx-get=(ViewerRoute::Refresh { tab })
            hx-target="#viewer-view"
            hx-sync="#viewer-view:replace"
            hx-swap="outerHTML" { "Refresh" }
    }
}

pub(super) fn delete_live_view_button(tab: ViewerTabId) -> Markup {
    delete_live_view_button_with_classes(
        tab,
        "viewer-control-button viewer-danger-button ml-2 inline-flex min-h-[27px] cursor-pointer items-center whitespace-nowrap rounded-sm border border-del-line bg-del-bg px-2 py-1 text-[11.5px] text-del [font:inherit] hover:border-del hover:bg-del hover:text-bg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-acc-line [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc",
    )
}

fn mobile_delete_live_view_button(tab: ViewerTabId) -> Markup {
    delete_live_view_button_with_classes(tab, preview::mobile_menu_danger_button_classes())
}

fn delete_live_view_button_with_classes(tab: ViewerTabId, classes: &str) -> Markup {
    html! {
        button type="button"
            class=(classes)
            aria-label="Delete saved live view"
            title="Delete saved live view"
            hx-delete=(ViewerRoute::DeleteLiveView { tab })
            hx-confirm="Delete this saved live view? This removes its tab and automatic restoration. You can add it again with gtl diff live."
            hx-target="#viewer-tabs"
            hx-sync="#viewer-view:replace"
            hx-swap="outerHTML" { "Delete live view" }
    }
}
