use std::{collections::HashMap, sync::Arc};

use application::{
    diffs::{FileDiff, View},
    viewer::{RenderOptions, ViewerTabId},
};
use domain::diffs::Commit;
use lru::LruCache;

/// A computed view and any server-rendered option variants retained with it.
#[derive(Debug, Clone)]
pub struct CachedView {
    pub(crate) view: Arc<View>,
    pub(crate) fragments: HashMap<RenderOptions, Arc<str>>,
    weight: usize,
}

impl CachedView {
    pub fn new(view: Arc<View>) -> Self {
        let weight = view_weight(&view);
        Self {
            view,
            fragments: HashMap::new(),
            weight,
        }
    }

    pub const fn weight(&self) -> usize {
        self.weight
    }

    fn insert_fragment(&mut self, options: RenderOptions, fragment: Arc<str>) {
        let inserted_weight = fragment.len();
        let replaced_weight = self
            .fragments
            .insert(options, fragment)
            .map_or(0, |replaced| replaced.len());
        self.weight = self
            .weight
            .checked_sub(replaced_weight)
            .and_then(|weight| weight.checked_add(inserted_weight))
            .expect("cached view fragment weights preserve the total");
    }
}

/// Whether an inserted value remains within the configured hard bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheDisposition {
    Cached,
    Oversize,
}

/// LRU storage bounded by the estimated bytes of views and rendered fragments.
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

    pub fn insert_fragment(
        &mut self,
        id: ViewerTabId,
        options: RenderOptions,
        fragment: Arc<str>,
    ) -> CacheDisposition {
        let Some(mut value) = self.entries.pop(&id) else {
            return CacheDisposition::Oversize;
        };
        self.weight -= value.weight();
        value.insert_fragment(options, fragment);
        self.insert(id, value)
    }

    pub(crate) fn get(&mut self, id: ViewerTabId) -> Option<&CachedView> {
        self.entries.get(&id)
    }

    pub fn remove(&mut self, id: ViewerTabId) -> Option<CachedView> {
        let removed = self.entries.pop(&id)?;
        self.weight -= removed.weight();
        Some(removed)
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
        + commit.members.iter().map(string_weight).sum::<usize>()
}

fn file_weight(file: &FileDiff) -> usize {
    string_weight(&file.path)
        + file.lines.iter().map(string_weight).sum::<usize>()
        + file
            .full_lines
            .as_ref()
            .map_or(0, |lines| lines.iter().map(string_weight).sum())
        + file.commits.iter().map(string_weight).sum::<usize>()
        + file
            .owners
            .added
            .values()
            .chain(file.owners.deleted.values())
            .map(string_weight)
            .sum::<usize>()
}

fn string_weight(value: &String) -> usize {
    value.capacity()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application::{
        diffs::{Cmd, Foot, LineOwners, View},
        viewer::{DiffDensity, DiffLayout, RenderOptions, ViewerTabId},
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
    fn fragment_bytes_contribute_to_the_weight_bound() {
        let value = cached("small");
        let base_weight = value.weight();
        let mut cache = WeightedViewCache::new(base_weight + 4);
        cache.insert(id(1), value);
        let options = RenderOptions::new(DiffLayout::Split, DiffDensity::Full);
        let fragment: Arc<str> = Arc::from("12345");

        assert_eq!(
            cache.insert_fragment(id(1), options, Arc::clone(&fragment)),
            CacheDisposition::Oversize
        );
        assert!(cache.get(id(1)).is_none());
        assert_eq!(&*fragment, "12345");
    }

    #[test]
    fn replacing_a_fragment_subtracts_its_previous_weight_before_reweighting() {
        let value = cached("small");
        let base_weight = value.weight();
        let mut cache = WeightedViewCache::new(base_weight + 4);
        cache.insert(id(1), value);
        let options = RenderOptions::new(DiffLayout::Split, DiffDensity::Full);
        assert_eq!(
            cache.insert_fragment(id(1), options, Arc::from("1234")),
            CacheDisposition::Cached
        );

        assert_eq!(
            cache.insert_fragment(id(1), options, Arc::from("12")),
            CacheDisposition::Cached
        );
        assert_eq!(cache.weight(), base_weight + 2);
        assert_eq!(
            cache
                .get(id(1))
                .expect("cached view")
                .fragments
                .get(&options)
                .expect("replacement fragment")
                .as_ref(),
            "12"
        );
    }

    #[test]
    fn fragments_under_distinct_options_are_kept_apart_and_weighed_together() {
        // One tab holds a fragment per live option set, so the weight has to sum
        // across keys rather than track the newest insertion.
        let value = cached("small");
        let base_weight = value.weight();
        let mut cache = WeightedViewCache::new(base_weight + 16);
        let compact = RenderOptions::DEFAULT;
        let full = compact.with_density(DiffDensity::Full);
        cache.insert(id(1), value);

        assert_eq!(
            cache.insert_fragment(id(1), compact, Arc::from("1234")),
            CacheDisposition::Cached
        );
        assert_eq!(
            cache.insert_fragment(id(1), full, Arc::from("123456")),
            CacheDisposition::Cached
        );

        assert_eq!(cache.weight(), base_weight + 10);
        let cached = cache.get(id(1)).expect("inserted entry stays cached");
        assert_eq!(
            cached.fragments.get(&compact).map(AsRef::as_ref),
            Some("1234")
        );
        assert_eq!(
            cached.fragments.get(&full).map(AsRef::as_ref),
            Some("123456")
        );
    }

    #[test]
    fn repeated_large_view_and_fragment_churn_never_crosses_the_hard_bound() {
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
                commits: vec![],
                owners: LineOwners::default(),
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

            let fragment: Arc<str> = Arc::from("x".repeat(512 * 1024));
            let _ = cache.insert_fragment(tab_id, RenderOptions::DEFAULT, fragment);
            assert!(cache.weight() <= crate::DEFAULT_VIEW_CACHE_WEIGHT);

            if raw_id > 2 {
                let _ = cache.get(id(raw_id - 2));
            }
        }
    }
}
