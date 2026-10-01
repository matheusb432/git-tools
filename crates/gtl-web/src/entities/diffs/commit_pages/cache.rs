use std::collections::VecDeque;

use dioxus::prelude::*;
use gtl_wire::viewer::ViewerViewIdentity;

use super::{ViewerCommitPageRequest, ViewerCommitPages};

const RETAINED_LISTS_MAX: usize = 8;
const RETAINED_BYTES_MAX: usize = 8 * 1024 * 1024;

struct CachedPages {
    pages: ViewerCommitPages,
    bytes: usize,
}

#[derive(Default)]
struct RetainedPages {
    instance: Option<String>,
    lists: VecDeque<CachedPages>,
    bytes: usize,
}

impl RetainedPages {
    fn restore(
        &mut self,
        instance: Option<&str>,
        identity: ViewerViewIdentity,
        expected: usize,
    ) -> ViewerCommitPages {
        if self.instance.as_deref() != instance {
            self.lists.clear();
            self.bytes = 0;
            self.instance = instance.map(str::to_owned);
        }
        let index = self.lists.iter().position(|list| {
            list.pages.key == identity.into() && list.pages.expected_count == expected
        });
        let Some(list) = index.and_then(|index| self.lists.remove(index)) else {
            return ViewerCommitPages::new(identity, expected);
        };
        self.bytes -= list.bytes;
        list.pages
    }

    fn retain(&mut self, instance: Option<&str>, pages: &ViewerCommitPages) {
        if self.instance.as_deref() != instance {
            return;
        }
        self.lists.retain(|list| list.pages.key != pages.key);
        self.bytes = self.lists.iter().map(|list| list.bytes).sum();
        let bytes = retained_bytes(pages);
        if bytes > RETAINED_BYTES_MAX || pages.commits.is_empty() {
            return;
        }
        while (self.bytes + bytes > RETAINED_BYTES_MAX || self.lists.len() >= RETAINED_LISTS_MAX)
            && let Some(evicted) = self.lists.pop_front()
        {
            self.bytes -= evicted.bytes;
        }
        let mut pages = pages.clone();
        pages.request = ViewerCommitPageRequest::Idle;
        self.lists.push_back(CachedPages { pages, bytes });
        self.bytes += bytes;
    }
}

fn retained_bytes(pages: &ViewerCommitPages) -> usize {
    std::mem::size_of_val(pages)
        + pages.commits.capacity() * std::mem::size_of::<gtl_wire::viewer::ViewerCommitSummary>()
        + pages
            .commits
            .iter()
            .map(|commit| {
                commit.body.capacity()
                    + commit.subject.capacity()
                    + commit.id.as_ref().len()
                    + commit.committed_at.as_ref().len()
            })
            .sum::<usize>()
}

#[derive(Clone, Copy)]
pub(super) struct CommitPageCache(Signal<RetainedPages>);

pub(crate) fn use_commit_page_cache_provider() {
    let pages = use_signal(RetainedPages::default);
    use_context_provider(|| CommitPageCache(pages));
}

impl CommitPageCache {
    pub(super) fn restore(
        mut self,
        instance: Option<&str>,
        identity: ViewerViewIdentity,
        expected: usize,
    ) -> ViewerCommitPages {
        self.0.write().restore(instance, identity, expected)
    }

    pub(super) fn retain(mut self, instance: Option<&str>, pages: &ViewerCommitPages) {
        self.0.write().retain(instance, pages);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::tests::{commit, identity},
        *,
    };

    #[test]
    fn restoring_a_tab_retains_its_page_cursor_and_retries_cancelled_work() {
        let identity = identity(0).unwrap();
        let mut pages = ViewerCommitPages::new(identity, 2);
        pages.commits.push(commit('a').unwrap());
        pages.next_cursor = Some(gtl_wire::viewer::ViewerCommitCursor::new(1));
        pages.request = ViewerCommitPageRequest::Loading;
        let mut cache = RetainedPages::default();
        cache.restore(Some("server"), identity, 2);
        cache.retain(Some("server"), &pages);
        let restored = cache.restore(Some("server"), identity, 2);
        assert_eq!(restored.commits, pages.commits);
        assert_eq!(restored.next_cursor, pages.next_cursor);
        assert_eq!(restored.request, ViewerCommitPageRequest::Idle);
        assert_eq!(cache.bytes, 0);
    }

    #[test]
    fn restart_or_oversize_pages_cannot_repopulate_the_cache() {
        let identity = identity(0).unwrap();
        let mut pages = ViewerCommitPages::new(identity, 1);
        pages.commits.push(commit('a').unwrap());
        let mut cache = RetainedPages::default();
        cache.restore(Some("before"), identity, 1);
        cache.retain(Some("before"), &pages);
        assert_eq!(
            cache.restore(Some("after"), identity, 1).commits,
            Vec::<gtl_wire::viewer::ViewerCommitSummary>::new()
        );
        assert_eq!(cache.bytes, 0);
        cache.retain(Some("before"), &pages);
        assert!(cache.lists.is_empty());
        pages.commits[0].body = "x".repeat(RETAINED_BYTES_MAX);
        cache.retain(Some("after"), &pages);
        assert!(cache.lists.is_empty());
    }
}
