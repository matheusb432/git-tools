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
    benchmark_materialized(
        c,
        BenchmarkCase::ViewerRenderDiffDocumentShell45k,
        BenchmarkCase::ViewerRenderMaterializedChunks45k,
        ViewerRenderBenchmark::fixture_45k,
    );
    benchmark_materialized(
        c,
        BenchmarkCase::ViewerRenderDiffDocumentShell115Files,
        BenchmarkCase::ViewerRenderMaterializedChunks115Files,
        ViewerRenderBenchmark::fixture_115_files,
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

fn benchmark_materialized(
    c: &mut Criterion,
    shell_case: BenchmarkCase,
    chunks_case: BenchmarkCase,
    fixture_factory: fn() -> ViewerRenderBenchmark,
) {
    let shell_fixture = OnceCell::new();
    let shell_label = shell_case.as_str();
    c.bench_function(shell_label, move |b| {
        let fixture = shell_fixture.get_or_init(|| {
            let fixture = fixture_factory();
            let shell = fixture.render_diff_document_shell(RenderOptions::DEFAULT);
            eprintln!("{shell_label} output_bytes={}", shell.len());
            fixture
        });
        b.iter(|| black_box(fixture.render_diff_document_shell(black_box(RenderOptions::DEFAULT))));
    });

    let chunks_fixture = OnceCell::new();
    let chunks_label = chunks_case.as_str();
    c.bench_function(chunks_label, move |b| {
        let fixture = chunks_fixture.get_or_init(|| {
            let fixture = fixture_factory();
            let chunks = fixture.render_chunks(RenderOptions::DEFAULT);
            eprintln!(
                "{chunks_label} chunks={} chunk_bytes={} max_chunk_bytes={}",
                chunks.len(),
                chunks.iter().map(|chunk| chunk.html.len()).sum::<usize>(),
                chunks
                    .iter()
                    .map(|chunk| chunk.html.len())
                    .max()
                    .unwrap_or(0),
            );
            fixture
        });
        b.iter(|| black_box(fixture.render_chunks(black_box(RenderOptions::DEFAULT))));
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(Benchmark::ViewerRender.sample_size());
    targets = render_viewer_boundaries
}
criterion_main!(benches);
