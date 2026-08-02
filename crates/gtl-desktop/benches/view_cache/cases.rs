use std::sync::Arc;

use gtl_application::{
    diffs::View,
    viewer::{RenderOptions, ViewerTabId},
};
use gtl_desktop::{CacheDisposition, CachedView, WeightedViewCache};

use super::view_fixture;

const FRAGMENT_BYTES: usize = 512 * 1024;

pub(super) struct CacheBenchmarkCase<F> {
    operation: F,
}

impl<F> CacheBenchmarkCase<F> {
    pub(super) fn execute<O>(&mut self) -> O
    where
        F: FnMut() -> O,
    {
        (self.operation)()
    }

    fn new(operation: F) -> Self {
        Self { operation }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ViewCacheBenchmark {
    view: Arc<View>,
    base: CachedView,
    with_fragment: CachedView,
    fragment: Arc<str>,
}

impl ViewCacheBenchmark {
    pub(super) fn fixture_small() -> Self {
        Self::new(Arc::new(view_fixture::view_with_lines(1)))
    }

    pub(super) fn fixture_45k() -> Self {
        Self::new(Arc::new(view_fixture::large_view()))
    }

    pub(super) fn view_weight_case(&self) -> CacheBenchmarkCase<impl FnMut() -> usize + use<>> {
        let mut view = Some(Arc::clone(&self.view));
        CacheBenchmarkCase::new(move || {
            CachedView::new(
                view.take()
                    .expect("each cache benchmark case executes exactly once"),
            )
            .weight()
        })
    }

    pub(super) fn fragment_insert_case(&self) -> CacheBenchmarkCase<impl FnMut() -> bool + use<>> {
        let max_weight = self
            .base
            .weight()
            .checked_add(self.fragment.len())
            .expect("benchmark cache weight fits usize");
        CacheBenchmarkCase::new(fragment_case(
            self.base.clone(),
            max_weight,
            Arc::clone(&self.fragment),
        ))
    }

    pub(super) fn fragment_replace_case(&self) -> CacheBenchmarkCase<impl FnMut() -> bool + use<>> {
        CacheBenchmarkCase::new(fragment_case(
            self.with_fragment.clone(),
            self.with_fragment.weight(),
            Arc::clone(&self.fragment),
        ))
    }

    pub(super) fn fragment_oversize_case(
        &self,
    ) -> CacheBenchmarkCase<impl FnMut() -> bool + use<>> {
        let max_weight = self
            .base
            .weight()
            .checked_add(self.fragment.len())
            .and_then(|weight| weight.checked_sub(1))
            .expect("benchmark fragment is non-empty and cache weight fits usize");
        CacheBenchmarkCase::new(fragment_case(
            self.base.clone(),
            max_weight,
            Arc::clone(&self.fragment),
        ))
    }

    pub(super) fn view_replace_case(&self) -> CacheBenchmarkCase<impl FnMut() -> bool + use<>> {
        let id = benchmark_tab_id();
        let mut cache = WeightedViewCache::new(self.base.weight());
        let _ = cache.insert(id, self.base.clone());
        let mut replacement = Some(self.base.clone());

        CacheBenchmarkCase::new(move || {
            cache.insert(
                id,
                replacement
                    .take()
                    .expect("each cache benchmark case executes exactly once"),
            ) == CacheDisposition::Cached
        })
    }

    fn new(view: Arc<View>) -> Self {
        let base = CachedView::new(Arc::clone(&view));
        let fragment: Arc<str> = Arc::from("x".repeat(FRAGMENT_BYTES));
        let with_fragment = cached_with_fragment(base.clone(), Arc::clone(&fragment));
        Self {
            view,
            base,
            with_fragment,
            fragment,
        }
    }
}

fn cached_with_fragment(base: CachedView, fragment: Arc<str>) -> CachedView {
    let id = benchmark_tab_id();
    let max_weight = base
        .weight()
        .checked_add(fragment.len())
        .expect("benchmark cache weight fits usize");
    let mut cache = WeightedViewCache::new(max_weight);
    let _ = cache.insert(id, base);
    let disposition = cache.insert_fragment(id, RenderOptions::DEFAULT, fragment);
    assert_eq!(disposition, CacheDisposition::Cached);
    cache
        .remove(id)
        .expect("cached benchmark fixture remains under its weight bound")
}

fn fragment_case(
    cached: CachedView,
    max_weight: usize,
    fragment: Arc<str>,
) -> impl FnMut() -> bool {
    let id = benchmark_tab_id();
    let mut cache = WeightedViewCache::new(max_weight);
    let _ = cache.insert(id, cached);
    let mut fragment = Some(fragment);

    move || {
        cache.insert_fragment(
            id,
            RenderOptions::DEFAULT,
            fragment
                .take()
                .expect("each cache benchmark case executes exactly once"),
        ) == CacheDisposition::Cached
    }
}

fn benchmark_tab_id() -> ViewerTabId {
    ViewerTabId::try_new(1).expect("benchmark tab id is positive")
}
