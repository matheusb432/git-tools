//! The palette picker: a tab-strip trigger and the panel it opens. The panel is
//! a top-layer `popover`, so it escapes the tab strip's horizontal scroll clip
//! and survives every fragment swap instead of being re-rendered with one.

use application::viewer::Theme;
use maud::{Markup, html};
use strum::VariantArray as _;

use crate::render::{ViewerRoute, ViewerSettingChange};

const POPOVER_ID: &str = "viewer-theme-popover";

/// Names one palette. The exhaustive match makes a new [`Theme`] variant a
/// compile error until it is named here.
pub(super) fn label(theme: Theme) -> &'static str {
    match theme {
        Theme::Dark => "Dark",
        Theme::Light => "Light",
        Theme::Hearth => "Hearth",
        Theme::Mirage => "Mirage",
        Theme::Glacier => "Glacier",
        Theme::Noir => "Noir",
        Theme::Graphite => "Graphite",
    }
}

pub(in crate::render) fn trigger(active: Theme) -> Markup {
    html! {
        button type="button"
            class="viewer-theme-button mb-[7px] flex flex-none cursor-pointer items-center gap-[7px] rounded-sm border border-transparent bg-transparent px-[9px] py-1.5 text-xs text-ink-2 [font:inherit] hover:border-line-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc mobile:hidden"
            popovertarget=(POPOVER_ID) {
            span class="sr-only" { "Theme: " }
            // The dot needs no update path: `--acc` re-resolves from the root
            // `data-theme` the moment the picked palette lands there.
            span class="size-[9px] flex-none rounded-full bg-acc" aria-hidden="true" {}
            span id="viewer-theme-name" { (label(active)) }
        }
    }
}

pub(in crate::render) fn popover(active: Theme) -> Markup {
    html! {
        // `top-[42px]` tracks the tab strip's rendered height, matching the
        // history popover's `inset-[42px]`. `bottom-auto left-auto` clears the
        // sides of the UA stylesheet's `inset: 0` that this panel does not set
        // itself; they are longhands, so no emission order can let them
        // override the `top` and `right` beside them. The palette list grows,
        // so the panel scrolls inside the viewport rather than past its bottom.
        div id=(POPOVER_ID)
            class="viewer-theme-popover fixed top-[42px] right-3 bottom-auto left-auto m-0 max-h-[calc(100vh_-_56px)] w-[188px] overflow-y-auto rounded-panel border border-line-2 bg-surface p-1.5 shadow-[0_18px_48px_rgba(0,0,0,.58)] mobile:right-2"
            role="group"
            aria-label="Theme"
            popover {
            @for theme in Theme::VARIANTS {
                (choice(active, *theme))
            }
        }
    }
}

fn choice(active: Theme, theme: Theme) -> Markup {
    html! {
        label class="viewer-theme-choice group relative block cursor-pointer" {
            input type="radio" class="peer pointer-events-none absolute size-px opacity-0" name="viewer-theme"
                data-viewer-theme=(theme)
                checked[active == theme]
                autofocus[active == theme]
                hx-get=(ViewerRoute::Settings(ViewerSettingChange::Theme(theme)))
                hx-trigger="change"
                hx-params="none"
                hx-swap="none";
            span class="flex min-h-[27px] items-center gap-2 rounded-sm px-2 py-1 text-[11.5px] group-hover:bg-surface-2 group-hover:text-ink peer-checked:bg-acc-soft peer-checked:text-acc peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-acc" {
                // The swatch carries its own palette, so it paints that theme's
                // accent while a different one is active on the document root.
                span class="size-[11px] flex-none rounded-full border border-line-2 bg-acc" data-theme=(theme) aria-hidden="true" {}
                span { (label(theme)) }
            }
        }
    }
}
