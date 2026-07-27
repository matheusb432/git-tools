//! Deterministic weighted view-cache benchmark cases.

use std::sync::Arc;

use application::{
    diffs::View,
    viewer::{RenderOptions, ViewerTabId},
};

use crate::session::{CacheDisposition, CachedView, WeightedViewCache};

const FRAGMENT_BYTES: usize = 512 * 1024;

/// Owns one prepared, one-shot weighted view-cache operation.
///
/// # Examples
///
/// ```
/// use desktop::benchmark_support::ViewCacheBenchmark;
///
/// let mut case = ViewCacheBenchmark::fixture_small().fragment_insert_case();
/// assert!(case.execute());
/// ```
pub struct CacheBenchmarkCase<F> {
    operation: F,
}

impl<F> CacheBenchmarkCase<F> {
    /// Executes the prepared cache operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use desktop::benchmark_support::ViewCacheBenchmark;
    ///
    /// let mut case = ViewCacheBenchmark::fixture_small().view_weight_case();
    /// assert!(case.execute() > 0);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the same one-shot case executes more than once.
    pub fn execute<O>(&mut self) -> O
    where
        F: FnMut() -> O,
    {
        (self.operation)()
    }

    fn new(operation: F) -> Self {
        Self { operation }
    }
}

/// Owns deterministic inputs for weighted view-cache benchmarks.
#[derive(Debug, Clone)]
pub struct ViewCacheBenchmark {
    base: CachedView,
    with_fragment: CachedView,
    fragment: Arc<str>,
}

impl ViewCacheBenchmark {
    /// Builds a minimal cache fixture.
    ///
    /// # Examples
    ///
    /// ```
    /// use desktop::benchmark_support::ViewCacheBenchmark;
    ///
    /// let fixture = ViewCacheBenchmark::fixture_small();
    /// assert!(fixture.fragment_replace_case().execute());
    /// ```
    #[must_use]
    pub fn fixture_small() -> Self {
        Self::new(Arc::new(super::view_with_lines(1)))
    }

    /// Builds the deterministic 45,000-line cache fixture.
    ///
    /// # Examples
    ///
    /// ```
    /// use desktop::benchmark_support::ViewCacheBenchmark;
    ///
    /// let fixture = ViewCacheBenchmark::fixture_45k();
    /// assert!(fixture.fragment_insert_case().execute());
    /// ```
    #[must_use]
    pub fn fixture_45k() -> Self {
        Self::new(Arc::new(super::large_view()))
    }

    /// Creates a one-shot case that computes the fixture's initial cache weight.
    ///
    /// # Examples
    ///
    /// ```
    /// use desktop::benchmark_support::ViewCacheBenchmark;
    ///
    /// let mut case = ViewCacheBenchmark::fixture_small().view_weight_case();
    /// assert!(case.execute() > 0);
    /// ```
    pub fn view_weight_case(&self) -> CacheBenchmarkCase<impl FnMut() -> usize + use<>> {
        let mut view = Some(Arc::clone(&self.base.view));
        CacheBenchmarkCase::new(move || {
            CachedView::new(
                view.take()
                    .expect("each cache benchmark case executes exactly once"),
            )
            .weight()
        })
    }

    /// Creates a one-shot case that inserts the fixture's first rendered fragment.
    ///
    /// # Examples
    ///
    /// ```
    /// use desktop::benchmark_support::ViewCacheBenchmark;
    ///
    /// let mut case = ViewCacheBenchmark::fixture_small().fragment_insert_case();
    /// assert!(case.execute());
    /// ```
    pub fn fragment_insert_case(&self) -> CacheBenchmarkCase<impl FnMut() -> bool + use<>> {
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

    /// Creates a one-shot case that replaces an equally sized rendered fragment.
    ///
    /// # Examples
    ///
    /// ```
    /// use desktop::benchmark_support::ViewCacheBenchmark;
    ///
    /// let mut case = ViewCacheBenchmark::fixture_small().fragment_replace_case();
    /// assert!(case.execute());
    /// ```
    pub fn fragment_replace_case(&self) -> CacheBenchmarkCase<impl FnMut() -> bool + use<>> {
        CacheBenchmarkCase::new(fragment_case(
            self.with_fragment.clone(),
            self.with_fragment.weight(),
            Arc::clone(&self.fragment),
        ))
    }

    /// Creates a one-shot case that rejects a fragment one byte above the cache bound.
    ///
    /// # Examples
    ///
    /// ```
    /// use desktop::benchmark_support::ViewCacheBenchmark;
    ///
    /// let mut case = ViewCacheBenchmark::fixture_small().fragment_oversize_case();
    /// assert!(!case.execute());
    /// ```
    pub fn fragment_oversize_case(&self) -> CacheBenchmarkCase<impl FnMut() -> bool + use<>> {
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

    /// Creates a one-shot case that replaces one whole cached view.
    ///
    /// # Examples
    ///
    /// ```
    /// use desktop::benchmark_support::ViewCacheBenchmark;
    ///
    /// let mut case = ViewCacheBenchmark::fixture_small().view_replace_case();
    /// assert!(case.execute());
    /// ```
    pub fn view_replace_case(&self) -> CacheBenchmarkCase<impl FnMut() -> bool + use<>> {
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
        let base = CachedView::new(view);
        let fragment: Arc<str> = Arc::from("x".repeat(FRAGMENT_BYTES));
        let with_fragment = cached_with_fragment(base.clone(), Arc::clone(&fragment));
        Self {
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
