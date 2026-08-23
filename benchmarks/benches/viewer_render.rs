use std::{cell::OnceCell, hint::black_box};

use criterion::{Criterion, criterion_group, criterion_main};
use gtl_application::viewer::{DiffDensity, DiffLayout, RenderOptions};

#[path = "viewer_render/fixture.rs"]
mod fixture;
#[path = "fixtures/view.rs"]
mod view_fixture;

use fixture::ViewerRenderBenchmark;

const RAW_ARTIFACT_BENCHMARK_NAME: &str = "raw-artifact";
const RAW_ARTIFACT_SPLIT_FULL_BENCHMARK_NAME: &str = "raw-artifact-split-full";
const SAMPLE_SIZE: usize = 10;

fn render_viewer_boundaries(c: &mut Criterion) {
    benchmark_raw_artifact(c, RAW_ARTIFACT_BENCHMARK_NAME, RenderOptions::DEFAULT);
    benchmark_raw_artifact(
        c,
        RAW_ARTIFACT_SPLIT_FULL_BENCHMARK_NAME,
        RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
    );
}

fn benchmark_raw_artifact(c: &mut Criterion, label: &'static str, options: RenderOptions) {
    let fixture = OnceCell::new();
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
    config = Criterion::default().sample_size(SAMPLE_SIZE);
    targets = render_viewer_boundaries
}
criterion_main!(benches);
