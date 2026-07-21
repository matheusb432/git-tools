//! The display controls header: layout/density radios scoped to the active
//! view, theme radios scoped to the app, and the live-view actions.

use application::viewer::{
    DiffDensity, DiffLayout, Theme, ViewerSettings, ViewerTabId, ViewerTabKind, ViewerView,
};
use maud::{Markup, html};

use crate::render::{ViewerRoute, ViewerSettingChange};

pub(super) fn view_controls(view: &ViewerView, settings: &ViewerSettings) -> Markup {
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
                (delete_live_view_button(view.tab_id()))
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

pub(super) fn delete_live_view_button(tab: ViewerTabId) -> Markup {
    html! {
        button type="button"
            class="viewer-control-button viewer-danger-button"
            aria-label="Delete saved live view"
            title="Delete saved live view"
            hx-delete=(ViewerRoute::DeleteLiveView { tab })
            hx-confirm="Delete this saved live view? This removes its tab and automatic restoration. You can add it again with gtl diff live."
            hx-target="#viewer-tabs"
            hx-swap="outerHTML" { "Delete live view" }
    }
}
