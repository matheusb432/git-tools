use std::{cell::OnceCell, hint::black_box};

use criterion::{Criterion, criterion_group, criterion_main};
use gtl_application::viewer::{DiffDensity, DiffLayout, RenderOptions};
use gtl_benchmarks::{Benchmark, BenchmarkCase};

#[path = "viewer_render/fixture.rs"]
mod fixture;
#[path = "fixtures/view.rs"]
mod view_fixture;

use fixture::ViewerRenderBenchmark;

fn render_viewer_boundaries(c: &mut Criterion) {
    benchmark_raw_artifact(
        c,
        BenchmarkCase::ViewerRenderRawArtifact,
        RenderOptions::DEFAULT,
    );
    benchmark_raw_artifact(
        c,
        BenchmarkCase::ViewerRenderRawArtifactSplitFull,
        RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
    );
}

fn benchmark_raw_artifact(c: &mut Criterion, case: BenchmarkCase, options: RenderOptions) {
    let fixture = OnceCell::new();
    let label = case.as_str();
    c.bench_function(label, move |b| {
        let fixture = fixture.get_or_init(|| {
            let fixture = ViewerRenderBenchmark::fixture_45k();
            eprintln!(
                "{label} output_bytes={}",
                fixture.render_raw_artifact(options).len()
            );
            fixture
        });
        b.iter(|| black_box(fixture.render_raw_artifact(black_box(options))));
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(Benchmark::ViewerRender.sample_size());
    targets = render_viewer_boundaries
}
criterion_main!(benches);
