use std::{hint::black_box, sync::Arc};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use gtl_application::{diffs::View, viewer::ViewerTabId};
use gtl_benchmarks::require;
use gtl_desktop::{CacheDisposition, CachedView, WeightedViewCache};

#[path = "fixtures/view.rs"]
mod view_fixture;

const VIEW_REPLACE_BENCHMARK_NAME: &str = "view-cache/view-replace/45k";
const VIEW_WEIGHT_BENCHMARK_NAME: &str = "view-cache/view-weight/45k";

fn cache_operations(criterion: &mut Criterion) {
    let view = Arc::new(view_fixture::large_view());

    criterion.bench_function(VIEW_WEIGHT_BENCHMARK_NAME, |bencher| {
        bencher.iter_batched_ref(
            || Arc::clone(&view),
            |view| black_box(CachedView::new(Arc::clone(view)).weight()),
            BatchSize::SmallInput,
        );
    });

    criterion.bench_function(VIEW_REPLACE_BENCHMARK_NAME, |bencher| {
        bencher.iter_batched(
            || view_replacement_fixture(&view),
            |(mut cache, id, replacement)| {
                black_box(cache.insert(id, replacement));
            },
            BatchSize::SmallInput,
        );
    });
}

fn view_replacement_fixture(view: &Arc<View>) -> (WeightedViewCache, ViewerTabId, CachedView) {
    let id = require(ViewerTabId::try_new(1), "creating a benchmark tab id");
    let cached = CachedView::new(Arc::clone(view));
    let mut cache = WeightedViewCache::new(cached.weight());
    assert_eq!(cache.insert(id, cached.clone()), CacheDisposition::Cached);
    (cache, id, cached)
}

criterion_group!(benches, cache_operations);
criterion_main!(benches);
