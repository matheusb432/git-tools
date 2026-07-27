//! Left sidebar: the changed-files tree with filter input and range stats.
//! The tree body itself is built client-side by the enhancement bundle from
//! the rendered file blocks.

use application::diffs::View;
use maud::{Markup, html};

use crate::text::plural;

const TREE_PRESENTATION_CLASSES: &str = concat!(
    "tree gtl-scroll [grid-area:2/1] overflow-auto border-r border-line bg-surface p-3 ",
    "[@media(max-width:1280px)]:p-2.5 [@media(max-width:1024px)]:hidden print:hidden! ",
    "[&_.tree-body_ul]:m-0 [&_.tree-body_ul]:list-none [&_.tree-body_ul]:pl-2.5 [&_.tree-body>ul]:pl-0 ",
    "[&_.tnode]:min-w-0 ",
    "[&_.tlabel]:flex [&_.tlabel]:cursor-pointer [&_.tlabel]:items-center [&_.tlabel]:gap-1.5 [&_.tlabel]:rounded-sm [&_.tlabel]:px-1.5 [&_.tlabel]:py-0.5 [&_.tlabel]:leading-[1.35] [&_.tlabel]:text-ink-2 ",
    "[&_.tdir>ul_.tfile>.tlabel]:pl-2 [&_.tlabel:hover]:bg-surface-2 [&_.tlabel:hover]:text-ink ",
    "[&_:is(.tdir>.tlabel,.tdir>.tlabel_.tname)]:text-ink-3 ",
    "[&_.tfile.cur>.tlabel]:bg-acc-soft [&_.tfile.cur>.tlabel]:text-ink [&_.tfile.cur>.tlabel]:shadow-[inset_2px_0_0_var(--acc)] ",
    "[&_.tcaret]:size-0 [&_.tcaret]:flex-none [&_.tcaret]:border-y-4 [&_.tcaret]:border-y-transparent [&_.tcaret]:border-l-5 [&_.tcaret]:border-l-ink-3 ",
    "[&_.tdir.open>.tlabel_.tcaret]:rotate-90 [&_.tdir:not(.open)>ul]:hidden ",
    "[&_.tname]:min-w-0 [&_.tname]:flex-1 [&_.tname]:overflow-hidden [&_.tname]:text-ellipsis ",
    "[&_.tfile.status-added>.tlabel]:bg-[color-mix(in_srgb,var(--add-bg)_42%,transparent)] ",
    "[&_.tfile.status-deleted>.tlabel]:bg-[color-mix(in_srgb,var(--del-bg)_42%,transparent)] ",
    "[&_.tfile.status-added>.tlabel:hover]:bg-[color-mix(in_srgb,var(--add-bg)_62%,var(--surface-2))] ",
    "[&_.tfile.status-deleted>.tlabel:hover]:bg-[color-mix(in_srgb,var(--del-bg)_62%,var(--surface-2))] ",
    "[&_.tstatus]:inline-flex [&_.tstatus]:size-[15px] [&_.tstatus]:flex-none [&_.tstatus]:items-center [&_.tstatus]:justify-center [&_.tstatus]:rounded-sm [&_.tstatus]:border [&_.tstatus]:border-line-2 [&_.tstatus]:text-[9.5px] [&_.tstatus]:leading-none [&_.tstatus]:font-bold ",
    "[&_.tstatus.status-added]:border-add-line [&_.tstatus.status-added]:bg-add-bg [&_.tstatus.status-added]:text-add ",
    "[&_.tstatus.status-deleted]:border-del-line [&_.tstatus.status-deleted]:bg-del-bg [&_.tstatus.status-deleted]:text-del ",
    "[&_.tstatus.status-renamed]:border-acc-line [&_.tstatus.status-renamed]:bg-acc-soft [&_.tstatus.status-renamed]:text-acc ",
    "[&_.tstatus.status-modified]:bg-sunk [&_.tstatus.status-modified]:text-ink-3",
);

// ! `.search` and `.tree-body` are enhancer hooks. The tree owns presentation for the
// ! client-rendered descendants beneath `.tree-body`.
pub(super) fn tree(view: &View) -> Markup {
    let total_add: u32 = view.files.iter().map(|f| f.added).sum();
    let total_del: u32 = view.files.iter().map(|f| f.removed).sum();
    let commit_count = view.commits.len();
    let file_count = view.files.len();
    // ! One border-color and one text-color utility per chip: stacking a neutral and an
    // ! accent utility of the same property leaves the winner to stylesheet order.
    let stat = "rounded-sm border px-2 py-0.5 text-[11px]";

    html! {
        aside class=(TREE_PRESENTATION_CLASSES) aria-label="Changed files tree" {
            div class="search relative mb-3 print:hidden!" {
                input type="text"
                    class="filter [font:inherit] w-full rounded-sm border border-line-2 bg-sunk px-2.5 py-2 text-[13px] text-ink focus:border-acc-line focus:shadow-[0_0_0_2px_var(--acc-soft)] focus:outline-none"
                    placeholder="Filter files…  /" aria-label="Filter files";
            }
            div class="mx-1 mt-1.5 mb-2 flex justify-between text-[11px] tracking-[0.06em] text-ink-3 uppercase" {
                span { (view.commits_label) " · " (file_count) " file" (plural(file_count)) }
            }
            div class="mx-0.5 mb-3 flex flex-wrap gap-2" {
                span class={ (stat) " border-line-2 text-ink-2" } { b class="font-bold text-ink" { (commit_count) } " commit" (plural(commit_count)) }
                span class={ (stat) " border-add-line text-add" } { "+" (total_add) }
                span class={ (stat) " border-del-line text-del" } { "−" (total_del) }
            }
            div class="tree-body text-[12.5px] whitespace-nowrap" {}
        }
    }
}

#[cfg(test)]
mod tests {
    use application::viewer::{RenderOptions, ViewerTabId};

    use crate::{fixtures::sample_view, view_fragment};

    #[test]
    fn tree_root_owns_client_rendered_node_presentation() {
        let tab_id = ViewerTabId::try_new(1).expect("positive tab id");
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT, tab_id)
            .into_string()
            .replace("&amp;", "&");
        let tree_classes = html
            .split_once(r#"<aside class="tree "#)
            .and_then(|(_, tail)| tail.split_once('"'))
            .map(|(classes, _)| classes)
            .expect("tree class attribute");

        for hook in [
            "[&_.tnode]",
            "[&_.tdir",
            "[&_.tfile",
            "[&_.tlabel]",
            "[&_.tcaret]",
            "[&_.tname]",
            "[&_.tstatus]",
        ] {
            assert!(tree_classes.contains(hook), "tree root must style `{hook}`");
        }
        assert!(html.contains(r#"class="search "#));
        assert!(html.contains(r#"class="tree-body "#));
    }
}
