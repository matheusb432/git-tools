//! Left sidebar: the changed-files tree with filter input and range stats.
//! The tree body itself is built client-side by the enhancement bundle from
//! the rendered file blocks.

use application::diffs::View;
use maud::{Markup, html};

use crate::text::plural;

pub(super) fn tree(view: &View) -> Markup {
    let total_add: u32 = view.files.iter().map(|f| f.added).sum();
    let total_del: u32 = view.files.iter().map(|f| f.removed).sum();
    let commit_count = view.commits.len();
    let file_count = view.files.len();

    html! {
        aside.tree aria-label="Changed files tree" {
            div.search {
                input type="text" class="filter" placeholder="Filter files…  /" aria-label="Filter files";
            }
            div.tree-head {
                span { (view.commits_label) " · " (file_count) " file" (plural(file_count)) }
            }
            div.stats {
                span.stat { b { (commit_count) } " commit" (plural(commit_count)) }
                span.stat.add { "+" (total_add) }
                span.stat.del { "−" (total_del) }
            }
            div.tree-body {}
        }
    }
}
