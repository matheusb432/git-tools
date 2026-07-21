//! Bottom keybar: the originating command line and keyboard hints.

use application::diffs::View;
use maud::{Markup, html};

pub(super) fn keybar(view: &View) -> Markup {
    html! {
        footer.keybar {
            span.cmd-line {
                (view.foot.cmd) " " span.dim { (view.foot.note) }
            }
            div.spacer {}
            span.key { kbd { "j" } " " kbd { "k" } " file" }
            span.key { kbd { "/" } " filter" }
            span.key { kbd { "alt+shift+c" } " fold all" }
        }
    }
}
