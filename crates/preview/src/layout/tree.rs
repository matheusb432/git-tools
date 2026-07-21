//! Left sidebar: the changed-files tree with filter input and range stats.
//! The tree body itself is built client-side by the enhancement bundle from
//! the rendered file blocks.

use application::diffs::View;
use maud::{Markup, html};

use crate::text::plural;

// ! `.tree` stays as the class anchor for the responsive column overrides and the print
// ! rule; `.search` and `.tree-body` are enhancer hooks (`.search input`, tree building).
// ! Rules for the tree body's file/dir nodes stay in styles/layout.css because that DOM is
// ! built client-side by the enhancement bundle, out of reach of template utilities.
pub(super) fn tree(view: &View) -> Markup {
    let total_add: u32 = view.files.iter().map(|f| f.added).sum();
    let total_del: u32 = view.files.iter().map(|f| f.removed).sum();
    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let stat = "rounded-sm border border-line-2 px-2 py-0.5 text-[11px] text-ink-2";

    html! {
        aside class="tree [grid-area:2/1] overflow-auto border-r border-line bg-surface p-[13px]" aria-label="Changed files tree" {
            div class="search relative mb-3" {
                input type="text"
                    class="filter [font:inherit] w-full rounded-sm border border-line-2 bg-sunk px-2.5 py-2 text-[13px] text-ink focus:border-acc-line focus:shadow-[0_0_0_2px_var(--acc-soft)] focus:outline-none"
                    placeholder="Filter files…  /" aria-label="Filter files";
            }
            div class="mx-1 mt-1.5 mb-2 flex justify-between text-[11px] tracking-[0.06em] text-ink-3 uppercase" {
                span { (view.commits_label) " · " (file_count) " file" (plural(file_count)) }
            }
            div class="mx-0.5 mb-3 flex flex-wrap gap-2" {
                span class=(stat) { b class="font-bold text-ink" { (commit_count) } " commit" (plural(commit_count)) }
                span class={ (stat) " border-add-line text-add" } { "+" (total_add) }
                span class={ (stat) " border-del-line text-del" } { "−" (total_del) }
            }
            div class="tree-body text-[12.5px] whitespace-nowrap" {}
        }
    }
}
