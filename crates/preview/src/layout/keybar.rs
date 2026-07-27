//! Bottom keybar: the originating command line and keyboard hints.

use application::diffs::View;
use maud::{Markup, html};

pub(super) fn keybar(view: &View) -> Markup {
    let kbd = "rounded-sm border border-line-2 border-b-2 bg-sunk px-1.5 py-px font-mono text-[11px] text-ink-2";

    html! {
        footer class="keybar [grid-column:1/4] flex items-center gap-4 overflow-hidden border-t border-line bg-surface px-5 py-2 text-[11.5px] text-ink-3 print:hidden!" {
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

#[cfg(test)]
mod tests {
    use application::viewer::{RenderOptions, ViewerTabId};

    use crate::{fixtures::sample_view, view_fragment};

    #[test]
    fn keybar_keeps_the_keyboard_command_contract() {
        let tab_id = ViewerTabId::try_new(1).expect("positive tab id");
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT, tab_id).into_string();

        assert!(html.contains(r#"<footer class="keybar "#));
        assert!(html.contains(">j</kbd> <kbd"));
        assert!(html.contains(">k</kbd> file"));
        assert!(html.contains(">/</kbd> filter"));
        assert!(html.contains(">alt+shift+c</kbd> fold all"));
    }
}
