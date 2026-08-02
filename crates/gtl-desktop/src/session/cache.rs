use std::sync::Arc;

use gtl_application::{
    diffs::{FileDiff, View},
    viewer::ViewerTabId,
};
use gtl_models::diffs::Commit;
use lru::LruCache;

/// A computed semantic view retained independently from rendered responses.
#[derive(Debug, Clone)]
pub struct CachedView {
    pub(crate) view: Arc<View>,
    pub(crate) selected: Option<Arc<View>>,
    weight: usize,
}

impl CachedView {
    pub fn new(view: Arc<View>) -> Self {
        let weight = view_weight(&view);
        Self {
            view,
            selected: None,
            weight,
        }
    }

    pub(crate) fn with_selected(&self, selected: Arc<View>) -> Self {
        Self {
            weight: self.weight + view_weight(&selected),
            view: Arc::clone(&self.view),
            selected: Some(selected),
        }
    }

    pub(crate) fn without_selected(&self) -> Self {
        Self::new(Arc::clone(&self.view))
    }

    pub const fn weight(&self) -> usize {
        self.weight
    }
}

/// Whether an inserted value remains within the configured hard bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheDisposition {
    Cached,
    Oversize,
}

/// LRU storage bounded by the estimated bytes of semantic views.
pub struct WeightedViewCache {
    entries: LruCache<ViewerTabId, CachedView>,
    max_weight: usize,
    weight: usize,
}

impl WeightedViewCache {
    pub fn new(max_weight: usize) -> Self {
        Self {
            entries: LruCache::unbounded(),
            max_weight,
            weight: 0,
        }
    }

    pub fn insert(&mut self, id: ViewerTabId, value: CachedView) -> CacheDisposition {
        self.remove(id);
        if value.weight() > self.max_weight {
            return CacheDisposition::Oversize;
        }

        self.weight += value.weight();
        self.entries.put(id, value);
        self.evict_to_bound();
        CacheDisposition::Cached
    }

    pub(crate) fn get(&mut self, id: ViewerTabId) -> Option<&CachedView> {
        self.entries.get(&id)
    }

    pub fn remove(&mut self, id: ViewerTabId) -> Option<()> {
        let removed = self.entries.pop(&id)?;
        self.weight -= removed.weight();
        Some(())
    }

    #[cfg(test)]
    pub(crate) const fn weight(&self) -> usize {
        self.weight
    }

    fn evict_to_bound(&mut self) {
        while self.weight > self.max_weight {
            let (_, evicted) = self
                .entries
                .pop_lru()
                .expect("positive cache weight has an entry");
            self.weight -= evicted.weight();
        }
    }
}

fn view_weight(view: &View) -> usize {
    string_weight(&view.repo_name)
        + string_weight(&view.repo_root)
        + string_weight(&view.branch)
        + string_weight(&view.upstream)
        + view.commits.iter().map(commit_weight).sum::<usize>()
        + view.files.iter().map(file_weight).sum::<usize>()
        + string_weight(&view.title)
        + string_weight(&view.cmd.lead)
        + string_weight(&view.cmd.range)
        + string_weight(&view.cmd.trail)
        + string_weight(&view.commits_label)
        + string_weight(&view.foot.cmd)
        + string_weight(&view.foot.note)
        + view.exclusions.as_ref().map_or(0, |applied| {
            applied
                .extensions
                .iter()
                .chain(&applied.hidden_paths)
                .map(string_weight)
                .sum()
        })
}

fn commit_weight(commit: &Commit) -> usize {
    string_weight(&commit.sha)
        + string_weight(&commit.subject)
        + string_weight(&commit.body)
        + string_weight(&commit.date)
        + string_weight(&commit.iso)
        + commit.parents.iter().map(string_weight).sum::<usize>()
}

fn file_weight(file: &FileDiff) -> usize {
    string_weight(&file.path)
        + file.lines.iter().map(string_weight).sum::<usize>()
        + file
            .full_lines
            .as_ref()
            .map_or(0, |lines| lines.iter().map(string_weight).sum())
}

fn string_weight(value: &String) -> usize {
    value.capacity()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gtl_application::{
        diffs::{Cmd, Foot, View},
        viewer::ViewerTabId,
    };

    use super::*;

    fn id(value: u64) -> ViewerTabId {
        ViewerTabId::try_new(value).expect("positive id")
    }

    fn cached(title: &str) -> CachedView {
        CachedView::new(Arc::new(View {
            exclusions: None,
            repo_name: String::new(),
            repo_root: String::new(),
            branch: String::new(),
            upstream: String::new(),
            commits: Vec::new(),
            files: Vec::new(),
            title: title.into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: String::new(),
            foot: Foot {
                cmd: String::new(),
                note: String::new(),
            },
        }))
    }

    #[test]
    fn cache_evicts_least_recent_tabs_before_crossing_weight_limit() {
        let first = cached("123456");
        let entry_weight = first.weight();
        let mut cache = WeightedViewCache::new(entry_weight + 1);
        cache.insert(id(1), first);
        cache.insert(id(2), cached("abcdef"));

        assert!(cache.get(id(1)).is_none());
        assert!(cache.get(id(2)).is_some());
        assert!(cache.weight() <= entry_weight + 1);
    }

    #[test]
    fn recently_read_entry_survives_the_next_eviction() {
        let entry_weight = cached("123456").weight();
        let mut cache = WeightedViewCache::new(entry_weight * 2);
        cache.insert(id(1), cached("123456"));
        cache.insert(id(2), cached("abcdef"));
        assert!(cache.get(id(1)).is_some());

        cache.insert(id(3), cached("ABCDEF"));

        assert!(cache.get(id(1)).is_some());
        assert!(cache.get(id(2)).is_none());
    }

    #[test]
    fn an_oversize_view_is_returned_but_not_cached() {
        let value = cached("oversize");
        let mut cache = WeightedViewCache::new(value.weight() - 1);

        assert_eq!(cache.insert(id(1), value), CacheDisposition::Oversize);
        assert!(cache.get(id(1)).is_none());
    }

    #[test]
    fn oversize_replacement_removes_the_previous_cached_view() {
        let small = cached("small");
        let max_weight = small.weight();
        let mut cache = WeightedViewCache::new(max_weight);
        assert_eq!(cache.insert(id(1), small), CacheDisposition::Cached);

        assert_eq!(
            cache.insert(id(1), cached("definitely oversize")),
            CacheDisposition::Oversize
        );
        assert!(cache.get(id(1)).is_none());
        assert_eq!(cache.weight(), 0);
    }

    #[test]
    fn repeated_large_view_churn_never_crosses_the_hard_bound() {
        const LINE_COUNT: usize = 45_000;
        let lines = std::iter::once(format!("@@ -1,{LINE_COUNT} +1,{LINE_COUNT} @@"))
            .chain((1..LINE_COUNT).map(|line| format!(" line {line:05}: cache churn payload")))
            .collect::<Vec<_>>();
        let view = Arc::new(View {
            exclusions: None,
            repo_name: "benchmark".into(),
            repo_root: "/fixtures/benchmark".into(),
            branch: "main".into(),
            upstream: "origin/main".into(),
            commits: vec![],
            files: vec![FileDiff {
                path: "src/large.rs".into(),
                added: 0,
                removed: 0,
                full_lines: Some(lines.clone()),
                lines,
            }],
            title: "Large diff".into(),
            cmd: Cmd {
                lead: "git diff ".into(),
                range: "origin/main..HEAD".into(),
                trail: String::new(),
            },
            commits_label: "0 commits".into(),
            foot: Foot {
                cmd: "git diff origin/main..HEAD".into(),
                note: "cache fixture".into(),
            },
        });
        let mut cache = WeightedViewCache::new(crate::DEFAULT_VIEW_CACHE_WEIGHT);

        for raw_id in 1..=96 {
            let tab_id = id(raw_id);
            assert_eq!(
                cache.insert(tab_id, CachedView::new(Arc::clone(&view))),
                CacheDisposition::Cached
            );
            assert!(cache.weight() <= crate::DEFAULT_VIEW_CACHE_WEIGHT);

            if raw_id > 2 {
                let _ = cache.get(id(raw_id - 2));
            }
        }
    }
}
