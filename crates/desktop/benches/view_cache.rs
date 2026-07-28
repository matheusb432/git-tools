use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

#[path = "view_cache/cases.rs"]
mod cases;
#[path = "fixtures/view.rs"]
mod view_fixture;

use cases::ViewCacheBenchmark;

fn cache_operations(c: &mut Criterion) {
    let small = ViewCacheBenchmark::fixture_small();
    let large = ViewCacheBenchmark::fixture_45k();

    c.bench_function("view-cache/view-weight/45k", |b| {
        b.iter_batched_ref(
            || large.view_weight_case(),
            |case| black_box(case.execute()),
            BatchSize::LargeInput,
        );
    });

    c.bench_function("view-cache/fragment-insert/45k", |b| {
        b.iter_batched_ref(
            || large.fragment_insert_case(),
            |case| black_box(case.execute()),
            BatchSize::LargeInput,
        );
    });

    c.bench_function("view-cache/fragment-replace/small", |b| {
        b.iter_batched_ref(
            || small.fragment_replace_case(),
            |case| black_box(case.execute()),
            BatchSize::LargeInput,
        );
    });

    c.bench_function("view-cache/fragment-replace/45k", |b| {
        b.iter_batched_ref(
            || large.fragment_replace_case(),
            |case| black_box(case.execute()),
            BatchSize::LargeInput,
        );
    });

    c.bench_function("view-cache/fragment-oversize/45k", |b| {
        b.iter_batched_ref(
            || large.fragment_oversize_case(),
            |case| black_box(case.execute()),
            BatchSize::LargeInput,
        );
    });

    c.bench_function("view-cache/view-replace/45k", |b| {
        b.iter_batched_ref(
            || large.view_replace_case(),
            |case| black_box(case.execute()),
            BatchSize::LargeInput,
        );
    });
}

criterion_group!(benches, cache_operations);
criterion_main!(benches);
