//! Bottom keybar: the originating command line and keyboard hints.

use application::diffs::View;
use maud::{Markup, html};

// ! `.keybar` stays as the class anchor for the print rule.
pub(super) fn keybar(view: &View) -> Markup {
    let kbd = "rounded-sm border border-line-2 border-b-2 bg-sunk px-1.5 py-px font-mono text-[11px] text-ink-2";

    html! {
        footer class="keybar [grid-column:1/4] flex items-center gap-4 overflow-hidden border-t border-line bg-surface px-5 py-2 text-[11.5px] text-ink-3" {
            span class="overflow-hidden text-ellipsis whitespace-nowrap text-ink-2" {
                (view.foot.cmd) " " span class="text-ink-3" { (view.foot.note) }
            }
            div class="flex-1" {}
            span class="flex-none" { kbd class=(kbd) { "j" } " " kbd class=(kbd) { "k" } " file" }
            span class="flex-none" { kbd class=(kbd) { "/" } " filter" }
            span class="flex-none" { kbd class=(kbd) { "alt+shift+c" } " fold all" }
        }
    }
}
