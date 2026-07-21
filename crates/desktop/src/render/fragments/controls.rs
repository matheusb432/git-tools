//! The display controls header: layout/density radios scoped to the active
//! view, theme radios scoped to the app, and the live-view actions.

use application::viewer::{
    DiffDensity, DiffLayout, Theme, ViewerSettings, ViewerTabId, ViewerTabKind, ViewerView,
};
use maud::{Markup, html};

use crate::render::{ViewerRoute, ViewerSettingChange};

pub(super) fn view_controls(view: &ViewerView, settings: &ViewerSettings) -> Markup {
    html! {
        header class="viewer-controls flex min-w-0 items-center gap-3 border-b border-line bg-surface px-3 py-[7px] text-ink-2 [@media(max-width:760px)]:gap-[7px] [@media(max-width:760px)]:overflow-x-auto" aria-label="Diff display controls" {
            div class="flex items-center gap-[3px]" role="group" aria-label="Layout" {
                span class="viewer-control-label mr-[3px] text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase [@media(max-width:760px)]:hidden" { "Layout" }
                (layout_choice(view, DiffLayout::Unified, "Unified"))
                (layout_choice(view, DiffLayout::Split, "Side by side"))
            }
            div class="flex items-center gap-[3px]" role="group" aria-label="Density" {
                span class="viewer-control-label mr-[3px] text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase [@media(max-width:760px)]:hidden" { "View" }
                (density_choice(view, DiffDensity::Compact, "Changes"))
                (density_choice(view, DiffDensity::Full, "Full file"))
            }
            @if view.kind() == ViewerTabKind::Live {
                button type="button"
                    class="viewer-control-button inline-flex min-h-[27px] cursor-pointer items-center whitespace-nowrap rounded-sm border border-transparent bg-transparent px-2 py-1 text-[11.5px] text-inherit [font:inherit] hover:border-line-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-acc-line [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc"
                    hx-get=(ViewerRoute::Refresh { tab: view.tab_id() })
                    hx-target="#viewer-view"
                    hx-swap="outerHTML" { "Refresh" }
                (delete_live_view_button(view.tab_id()))
            }
            div class="flex-1" {}
            div class="flex items-center gap-[3px]" role="group" aria-label="Theme" {
                span class="viewer-control-label mr-[3px] text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase [@media(max-width:760px)]:hidden" { "Theme" }
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
        label class="viewer-choice group relative cursor-pointer"
            hx-get=(ViewerRoute::View { tab: view.tab_id(), options })
            hx-trigger="change from:find input"
            hx-params="none"
            hx-target="#viewer-view"
            hx-swap="outerHTML" {
            input type="radio" class="peer pointer-events-none absolute size-px opacity-0" name="viewer-layout" value=(layout)
                checked[view.options().layout() == layout]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Layout(layout)))
                hx-trigger="change"
                hx-params="none"
                hx-swap="none";
            span class="inline-flex min-h-[27px] items-center whitespace-nowrap rounded-sm border border-transparent px-2 py-1 text-[11.5px] group-hover:border-line-2 group-hover:bg-surface-2 group-hover:text-ink peer-checked:border-acc-line peer-checked:bg-acc-soft peer-checked:text-acc peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-acc" { (label) }
        }
    }
}

fn density_choice(view: &ViewerView, density: DiffDensity, label: &str) -> Markup {
    let options = view.options().with_density(density);
    html! {
        label class="viewer-choice group relative cursor-pointer"
            hx-get=(ViewerRoute::View { tab: view.tab_id(), options })
            hx-trigger="change from:find input"
            hx-params="none"
            hx-target="#viewer-view"
            hx-swap="outerHTML" {
            input type="radio" class="peer pointer-events-none absolute size-px opacity-0" name="viewer-density" value=(density)
                checked[view.options().density() == density]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Density(density)))
                hx-trigger="change"
                hx-params="none"
                hx-swap="none";
            span class="inline-flex min-h-[27px] items-center whitespace-nowrap rounded-sm border border-transparent px-2 py-1 text-[11.5px] group-hover:border-line-2 group-hover:bg-surface-2 group-hover:text-ink peer-checked:border-acc-line peer-checked:bg-acc-soft peer-checked:text-acc peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-acc" { (label) }
        }
    }
}

fn theme_choice(settings: &ViewerSettings, theme: Theme, label: &str) -> Markup {
    html! {
        label class="viewer-choice group relative cursor-pointer" {
            input type="radio" class="peer pointer-events-none absolute size-px opacity-0" name="viewer-theme" value=(theme)
                data-viewer-theme=(theme)
                checked[settings.theme() == theme]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Theme(theme)))
                hx-trigger="change"
                hx-params="none"
                hx-swap="none";
            span class="inline-flex min-h-[27px] items-center whitespace-nowrap rounded-sm border border-transparent px-2 py-1 text-[11.5px] group-hover:border-line-2 group-hover:bg-surface-2 group-hover:text-ink peer-checked:border-acc-line peer-checked:bg-acc-soft peer-checked:text-acc peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-acc" { (label) }
        }
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
