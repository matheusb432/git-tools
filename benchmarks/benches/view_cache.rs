use std::{hint::black_box, sync::Arc};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use gtl_application::{diffs::View, viewer::ViewerTabId};
use gtl_benchmarks::{Benchmark, BenchmarkCase, require};
use gtl_desktop::{CacheDisposition, CachedView, WeightedViewCache};

#[path = "fixtures/view.rs"]
mod view_fixture;

fn cache_operations(criterion: &mut Criterion) {
    let view = Arc::new(view_fixture::large_view());

    criterion.bench_function(BenchmarkCase::ViewCacheViewWeight45k.as_str(), |bencher| {
        bencher.iter_batched_ref(
            || Arc::clone(&view),
            |view| black_box(CachedView::new(Arc::clone(view)).weight()),
            BatchSize::SmallInput,
        );
    });

    criterion.bench_function(BenchmarkCase::ViewCacheViewReplace45k.as_str(), |bencher| {
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

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(Benchmark::ViewCache.sample_size());
    targets = cache_operations
}
criterion_main!(benches);
