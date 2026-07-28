//! The display controls header: the layout and density radios scoped to the
//! active view, plus the live-view actions.

use application::viewer::{DiffDensity, DiffLayout, Theme, ViewerTabId, ViewerTabKind, ViewerView};
use maud::{Markup, html};

use crate::render::{ViewerRoute, ViewerSettingChange};

pub(super) fn view_controls(view: &ViewerView, active_theme: Theme) -> Markup {
    html! {
        header class="viewer-controls gtl-scroll flex min-w-0 items-center gap-3 border-b border-line bg-surface px-3 py-[7px] text-ink-2 [@media(max-width:760px)]:hidden" aria-label="Diff display controls" {
            div class="flex items-center gap-[3px]" role="group" aria-label="Layout" {
                span class="viewer-control-label mr-[3px] text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase" { "Layout" }
                (layout_choice(view, DiffLayout::Unified, "Unified", "viewer-layout"))
                (layout_choice(view, DiffLayout::Split, "Side by side", "viewer-layout"))
            }
            div class="flex items-center gap-[3px]" role="group" aria-label="Density" {
                span class="viewer-control-label mr-[3px] text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase" { "View" }
                (density_choice(view, DiffDensity::Compact, "Changes", "viewer-density"))
                (density_choice(view, DiffDensity::Full, "Full file", "viewer-density"))
            }
            @if view.kind() == ViewerTabKind::Live {
                (refresh_button(view.tab_id()))
                (delete_live_view_button(view.tab_id()))
            }
        }
        aside id="viewer-controls-popover"
            class="viewer-controls-popover fixed inset-3 m-0 ml-auto h-[calc(100vh_-_24px)] w-[min(330px,calc(100vw_-_24px))] max-w-none overflow-y-auto rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-[0_24px_80px_rgba(0,0,0,.72)] [&::backdrop]:bg-[rgba(0,0,0,.42)]"
            popover {
            header class="flex items-center justify-between border-b border-line bg-surface-2 px-4 py-3" {
                strong class="text-[13px]" { "View settings" }
                button type="button" class="size-[30px] cursor-pointer rounded-sm border-0 bg-transparent text-xl text-ink-2 [font:inherit] hover:bg-line hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc"
                    popovertarget="viewer-controls-popover" popovertargetaction="hide" aria-label="Close view settings" { "×" }
            }
            div class="space-y-5 p-4" {
                fieldset class="m-0 border-0 p-0" {
                    legend class="mb-2 text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase" { "Layout" }
                    div class="grid grid-cols-2 gap-2" {
                        (layout_choice(view, DiffLayout::Unified, "Unified", "viewer-layout-mobile"))
                        (layout_choice(view, DiffLayout::Split, "Side by side", "viewer-layout-mobile"))
                    }
                }
                fieldset class="m-0 border-0 p-0" {
                    legend class="mb-2 text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase" { "Density" }
                    div class="grid grid-cols-2 gap-2" {
                        (density_choice(view, DiffDensity::Compact, "Changes", "viewer-density-mobile"))
                        (density_choice(view, DiffDensity::Full, "Full file", "viewer-density-mobile"))
                    }
                }
                div class="grid gap-2" aria-label="Preview actions" {
                    (menu_action_button("Collapse or expand all files", "fold-all"))
                    (menu_action_button("Toggle copy context", "toggle-context"))
                    button type="button"
                        class=(MENU_BUTTON_CLASSES)
                        hx-get=(ViewerRoute::History)
                        hx-target="#viewer-history"
                        hx-swap="outerHTML"
                        popovertarget="viewer-history-popover" { "Render history" }
                    button type="button" class=(MENU_BUTTON_CLASSES) popovertarget="viewer-theme-popover" {
                        "Theme · " (super::theme::label(active_theme))
                    }
                    @if view.kind() == ViewerTabKind::Live {
                        (menu_action_button("Refresh", "refresh"))
                        (menu_action_button("Delete live view", "delete-live-view"))
                    }
                }
            }
        }
    }
}

const MENU_BUTTON_CLASSES: &str = "viewer-control-button inline-flex min-h-[38px] w-full cursor-pointer items-center rounded-sm border border-line-2 bg-surface-2 px-3 py-2 text-left text-[12px] text-ink-2 [font:inherit] hover:border-acc-line hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc";

fn layout_choice(view: &ViewerView, layout: DiffLayout, label: &str, input_name: &str) -> Markup {
    let options = view.options().with_layout(layout);
    html! {
        label class="viewer-choice group relative cursor-pointer"
            hx-get=(ViewerRoute::View { tab: view.tab_id(), options })
            hx-trigger="change from:find input"
            hx-params="none"
            hx-target="#viewer-view"
            hx-swap="outerHTML" {
            input type="radio" class="peer pointer-events-none absolute size-px opacity-0" name=(input_name) value=(layout)
                checked[view.options().layout() == layout]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Layout(layout)))
                hx-trigger="change"
                hx-params="none"
                hx-swap="none";
            span class="inline-flex min-h-[27px] items-center whitespace-nowrap rounded-sm border border-transparent px-2 py-1 text-[11.5px] group-hover:border-line-2 group-hover:bg-surface-2 group-hover:text-ink peer-checked:border-acc-line peer-checked:bg-acc-soft peer-checked:text-acc peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-acc" { (label) }
        }
    }
}

fn density_choice(
    view: &ViewerView,
    density: DiffDensity,
    label: &str,
    input_name: &str,
) -> Markup {
    let options = view.options().with_density(density);
    html! {
        label class="viewer-choice group relative cursor-pointer"
            hx-get=(ViewerRoute::View { tab: view.tab_id(), options })
            hx-trigger="change from:find input"
            hx-params="none"
            hx-target="#viewer-view"
            hx-swap="outerHTML" {
            input type="radio" class="peer pointer-events-none absolute size-px opacity-0" name=(input_name) value=(density)
                checked[view.options().density() == density]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Density(density)))
                hx-trigger="change"
                hx-params="none"
                hx-swap="none";
            span class="inline-flex min-h-[27px] items-center whitespace-nowrap rounded-sm border border-transparent px-2 py-1 text-[11.5px] group-hover:border-line-2 group-hover:bg-surface-2 group-hover:text-ink peer-checked:border-acc-line peer-checked:bg-acc-soft peer-checked:text-acc peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-acc" { (label) }
        }
    }
}

fn menu_action_button(label: &str, action: &str) -> Markup {
    html! {
        button type="button" class=(MENU_BUTTON_CLASSES) data-mobile-preview-action=(action) { (label) }
    }
}

fn refresh_button(tab: ViewerTabId) -> Markup {
    html! {
        button type="button"
            class="viewer-control-button inline-flex min-h-[27px] cursor-pointer items-center whitespace-nowrap rounded-sm border border-transparent bg-transparent px-2 py-1 text-[11.5px] text-inherit [font:inherit] hover:border-line-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-acc-line [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc"
            hx-get=(ViewerRoute::Refresh { tab })
            hx-target="#viewer-view"
            hx-swap="outerHTML" { "Refresh" }
    }
}

pub(super) fn delete_live_view_button(tab: ViewerTabId) -> Markup {
    html! {
        button type="button"
            class="viewer-control-button viewer-danger-button ml-2 inline-flex min-h-[27px] cursor-pointer items-center whitespace-nowrap rounded-sm border border-del-line bg-del-bg px-2 py-1 text-[11.5px] text-del [font:inherit] hover:border-del hover:bg-del hover:text-bg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-acc-line [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc"
            aria-label="Delete saved live view"
            title="Delete saved live view"
            hx-delete=(ViewerRoute::DeleteLiveView { tab })
            hx-confirm="Delete this saved live view? This removes its tab and automatic restoration. You can add it again with gtl diff live."
            hx-target="#viewer-tabs"
            hx-swap="outerHTML" { "Delete live view" }
    }
}
