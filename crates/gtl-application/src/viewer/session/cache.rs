use std::{iter::Sum, ops::Add, path::Path, sync::Arc};

use gtl_models::{
    diffs::{Commit, DiffViewTitle},
    git::{GitHead, GitRevision},
};
use lru::LruCache;

use crate::{
    diffs::{FileDiff, FullContextDiffState, View},
    viewer::{ViewerDiffSnapshot, ViewerTabId},
};

/// Estimated retained bytes used to bound the semantic viewer cache.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewCacheWeight(usize);

impl ViewCacheWeight {
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn bytes(self) -> usize {
        self.0
    }

    #[must_use]
    const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    #[must_use]
    const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    #[cfg(test)]
    #[must_use]
    const fn saturating_mul(self, multiplier: usize) -> Self {
        Self(self.0.saturating_mul(multiplier))
    }
}

impl Add for ViewCacheWeight {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        self.saturating_add(other)
    }
}

impl Sum for ViewCacheWeight {
    fn sum<I: Iterator<Item = Self>>(weights: I) -> Self {
        weights.fold(Self::default(), Self::saturating_add)
    }
}

/// A computed semantic view retained independently from rendered responses.
#[derive(Debug, Clone)]
pub struct CachedView {
    pub view: ViewerDiffSnapshot,
    pub selected: Option<ViewerDiffSnapshot>,
    pub modified: Option<ViewerDiffSnapshot>,
    weight: ViewCacheWeight,
    base_weight: ViewCacheWeight,
}

impl CachedView {
    #[must_use]
    pub fn new(view: Arc<View>) -> Self {
        Self::from_snapshot(ViewerDiffSnapshot::new(view))
    }

    pub(in crate::viewer) fn from_snapshot(view: ViewerDiffSnapshot) -> Self {
        let weight = view_weight(&view);
        Self {
            view,
            selected: None,
            modified: None,
            weight,
            base_weight: weight,
        }
    }

    #[must_use]
    pub fn with_selected(&self, selected: ViewerDiffSnapshot) -> Self {
        Self {
            weight: self.base_weight
                + view_weight(&selected)
                + self
                    .modified
                    .as_ref()
                    .map_or(ViewCacheWeight::default(), |view| view_weight(view)),
            modified: self.modified.clone(),
            base_weight: self.base_weight,
            view: self.view.clone(),
            selected: Some(selected),
        }
    }

    #[must_use]
    pub fn without_selected(&self) -> Self {
        Self {
            view: self.view.clone(),
            selected: None,
            weight: self.base_weight
                + self
                    .modified
                    .as_ref()
                    .map_or(ViewCacheWeight::default(), |view| view_weight(view)),
            modified: self.modified.clone(),
            base_weight: self.base_weight,
        }
    }

    pub(in crate::viewer) fn with_modified(&self, modified: Option<ViewerDiffSnapshot>) -> Self {
        Self {
            view: self.view.clone(),
            selected: self.selected.clone(),
            weight: self.base_weight
                + self
                    .selected
                    .as_ref()
                    .map_or(ViewCacheWeight::default(), |view| view_weight(view))
                + modified
                    .as_ref()
                    .map_or(ViewCacheWeight::default(), |view| view_weight(view)),
            base_weight: self.base_weight,
            modified,
        }
    }

    #[must_use]
    pub const fn weight(&self) -> ViewCacheWeight {
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
    max_weight: ViewCacheWeight,
    weight: ViewCacheWeight,
}

impl WeightedViewCache {
    #[must_use]
    pub fn new(max_weight: ViewCacheWeight) -> Self {
        Self {
            entries: LruCache::unbounded(),
            max_weight,
            weight: ViewCacheWeight::default(),
        }
    }

    pub fn insert(&mut self, id: ViewerTabId, value: CachedView) -> CacheDisposition {
        self.remove(id);
        if value.weight() > self.max_weight {
            return CacheDisposition::Oversize;
        }

        self.weight = self.weight.saturating_add(value.weight());
        self.entries.put(id, value);
        self.evict_to_bound();
        CacheDisposition::Cached
    }

    pub fn get(&mut self, id: ViewerTabId) -> Option<&CachedView> {
        self.entries.get(&id)
    }

    pub(super) fn peek(&self, id: ViewerTabId) -> Option<&CachedView> {
        self.entries.peek(&id)
    }

    pub fn remove(&mut self, id: ViewerTabId) -> Option<()> {
        let removed = self.entries.pop(&id)?;
        self.weight = self.weight.saturating_sub(removed.weight());
        Some(())
    }

    #[cfg(test)]
    #[must_use]
    pub const fn weight(&self) -> ViewCacheWeight {
        self.weight
    }

    fn evict_to_bound(&mut self) {
        while self.weight > self.max_weight {
            self.evict_lru();
        }
    }

    fn evict_lru(&mut self) {
        match self.entries.pop_lru() {
            Some((_, evicted)) => {
                self.weight = self.weight.saturating_sub(evicted.weight());
            }
            None => self.weight = ViewCacheWeight::default(),
        }
    }
}

fn view_weight(view: &View) -> ViewCacheWeight {
    ViewCacheWeight::new(view.repo_name.as_str().len())
        + path_weight(view.repo_root.as_ref())
        + head_weight(&view.branch)
        + revision_weight(&view.upstream)
        + view
            .commits
            .iter()
            .map(commit_weight)
            .sum::<ViewCacheWeight>()
        + view.files.iter().map(file_weight).sum::<ViewCacheWeight>()
        + view
            .file_filter
            .hidden_files()
            .iter()
            .map(file_weight)
            .sum::<ViewCacheWeight>()
        + view
            .file_filter
            .filter()
            .extensions()
            .extensions()
            .iter()
            .map(string_weight)
            .sum::<ViewCacheWeight>()
        + title_weight(&view.title)
        + string_weight(&view.cmd.lead)
        + string_weight(&view.cmd.range)
        + string_weight(&view.cmd.trail)
        + string_weight(&view.foot.cmd)
        + full_context_weight(&view.full_context)
        + view
            .extension_filter
            .as_ref()
            .map_or(ViewCacheWeight::default(), |applied| {
                applied
                    .filter
                    .extensions()
                    .extensions()
                    .iter()
                    .map(string_weight)
                    .sum::<ViewCacheWeight>()
                    + applied
                        .hidden_paths
                        .iter()
                        .map(|path| path_weight(path.as_path()))
                        .sum()
            })
}

fn full_context_weight(state: &FullContextDiffState) -> ViewCacheWeight {
    match state {
        FullContextDiffState::Deferred(source) => {
            ViewCacheWeight::new(source.spec().as_arg().len())
                + source
                    .paths()
                    .extensions()
                    .extensions()
                    .iter()
                    .map(string_weight)
                    .sum()
        }
        FullContextDiffState::Unavailable | FullContextDiffState::Loaded => {
            ViewCacheWeight::default()
        }
    }
}

fn commit_weight(commit: &Commit) -> ViewCacheWeight {
    ViewCacheWeight::new(commit.id.as_ref().len())
        + string_weight(&commit.subject)
        + string_weight(&commit.body)
        + ViewCacheWeight::new(commit.committed_at.as_ref().len())
        + commit
            .parents
            .iter()
            .map(|parent| ViewCacheWeight::new(parent.as_ref().len()))
            .sum::<ViewCacheWeight>()
}

fn file_weight(file: &FileDiff) -> ViewCacheWeight {
    path_weight(file.path.as_path())
        + ViewCacheWeight::new(file.lines.retained_bytes())
        + file
            .full_lines
            .as_ref()
            .map_or(ViewCacheWeight::default(), |lines| {
                ViewCacheWeight::new(lines.retained_bytes())
            })
}

fn title_weight(title: &DiffViewTitle) -> ViewCacheWeight {
    match title {
        DiffViewTitle::Diff | DiffViewTitle::MergeDiff => ViewCacheWeight::default(),
        DiffViewTitle::Commit { id } => ViewCacheWeight::new(id.as_ref().len()),
        DiffViewTitle::Named { name } => ViewCacheWeight::new(name.as_str().len()),
    }
}

fn string_weight(value: &String) -> ViewCacheWeight {
    ViewCacheWeight::new(value.capacity())
}

fn path_weight(value: &Path) -> ViewCacheWeight {
    ViewCacheWeight::new(value.as_os_str().len())
}

fn head_weight(head: &GitHead) -> ViewCacheWeight {
    ViewCacheWeight::new(head.branch().map_or(0, |branch| branch.capacity()))
}

fn revision_weight(revision: &GitRevision) -> ViewCacheWeight {
    ViewCacheWeight::new(revision.capacity())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gtl_models::diffs::DiffLineCount;

    use super::*;
    use crate::{
        diffs::{Cmd, Foot, View},
        utils::{git_head, git_revision, project_name, repository_relative_path, repository_root},
        viewer::ViewerTabId,
    };

    fn id(value: u64) -> ViewerTabId {
        ViewerTabId::try_new(value).unwrap()
    }

    fn cached(title: &str) -> CachedView {
        CachedView::new(Arc::new(View {
            file_filter: crate::diffs::file_filter::DiffFileFilter::default(),
            extension_filter: None,
            repo_name: project_name("repo"),
            repo_root: repository_root("/repo"),
            branch: GitHead::Detached,
            upstream: GitRevision::head(),
            commits: Vec::new(),
            files: Vec::new(),
            title: crate::utils::diffs::view_title(title),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            foot: Foot { cmd: String::new() },
            full_context: crate::diffs::FullContextDiffState::Unavailable,
        }))
    }

    fn touch_older_entry(cache: &mut WeightedViewCache, raw_id: u64) {
        let Some(older_id) = raw_id.checked_sub(2).filter(|older_id| *older_id > 0) else {
            return;
        };
        let _ = cache.get(id(older_id));
    }

    #[test]
    fn replacing_a_selected_patch_counts_only_the_retained_patch() {
        let range = cached("range");
        let patch = cached("patch").view;
        let selected = range.with_selected(patch.clone());
        let replaced = selected.with_selected(patch);
        assert_eq!(replaced.weight(), selected.weight());
        assert_eq!(replaced.without_selected().weight(), range.weight());
    }

    #[test]
    fn cache_evicts_least_recent_tabs_before_crossing_weight_limit() {
        let first = cached("123456");
        let entry_weight = first.weight();
        let bound = entry_weight.saturating_add(ViewCacheWeight::new(1));
        let mut cache = WeightedViewCache::new(bound);
        cache.insert(id(1), first);
        cache.insert(id(2), cached("abcdef"));

        assert!(cache.get(id(1)).is_none());
        assert!(cache.get(id(2)).is_some());
        assert!(cache.weight() <= bound);
    }

    #[test]
    fn recently_read_entry_survives_the_next_eviction() {
        let entry_weight = cached("123456").weight();
        let mut cache = WeightedViewCache::new(entry_weight.saturating_mul(2));
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
        let mut cache =
            WeightedViewCache::new(value.weight().saturating_sub(ViewCacheWeight::new(1)));

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
        assert_eq!(cache.weight(), ViewCacheWeight::default());
    }

    #[test]
    fn repeated_large_view_churn_never_crosses_the_hard_bound() {
        const LINE_COUNT: usize = 45_000;
        let lines = std::iter::once(format!("@@ -1,{LINE_COUNT} +1,{LINE_COUNT} @@"))
            .chain((1..LINE_COUNT).map(|line| format!(" line {line:05}: cache churn payload")))
            .collect::<Vec<_>>();
        let view = Arc::new(View {
            file_filter: crate::diffs::file_filter::DiffFileFilter::default(),
            extension_filter: None,
            repo_name: project_name("benchmark"),
            repo_root: repository_root("/fixtures/benchmark"),
            branch: git_head("main"),
            upstream: git_revision("origin/main"),
            commits: vec![],
            files: vec![FileDiff {
                path: repository_relative_path("src/large.rs"),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                full_lines: Some(lines.clone().into()),
                lines: lines.into(),
            }],
            title: crate::utils::diffs::view_title("Large diff"),
            cmd: Cmd {
                lead: "git diff ".into(),
                range: "origin/main..HEAD".into(),
                trail: String::new(),
            },
            foot: Foot {
                cmd: "git diff origin/main..HEAD".into(),
            },
            full_context: crate::diffs::FullContextDiffState::Loaded,
        });
        let mut cache = WeightedViewCache::new(super::super::DEFAULT_VIEW_CACHE_WEIGHT);

        for raw_id in 1..=96 {
            let tab_id = id(raw_id);
            assert_eq!(
                cache.insert(tab_id, CachedView::new(Arc::clone(&view))),
                CacheDisposition::Cached
            );
            assert!(cache.weight() <= super::super::DEFAULT_VIEW_CACHE_WEIGHT);

            touch_older_entry(&mut cache, raw_id);
        }
    }
}
