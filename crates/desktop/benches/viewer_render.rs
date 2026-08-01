use std::{cell::OnceCell, hint::black_box};

use application::viewer::{DiffDensity, DiffLayout, RenderOptions};
use criterion::{Criterion, criterion_group, criterion_main};

#[path = "viewer_render/fixture.rs"]
mod fixture;
#[path = "fixtures/view.rs"]
mod view_fixture;

use fixture::ViewerRenderBenchmark;

fn render_large_viewer(c: &mut Criterion) {
    benchmark_view(
        c,
        "unified-compact",
        RenderOptions::new(DiffLayout::Unified, DiffDensity::Compact),
    );
    benchmark_view(
        c,
        "split-full",
        RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
    );
    benchmark_raw_artifact(c);
    benchmark_materialized(c, "45k", ViewerRenderBenchmark::fixture_45k);
    benchmark_materialized(c, "115-files", ViewerRenderBenchmark::fixture_115_files);
}

fn benchmark_view(c: &mut Criterion, label: &'static str, options: RenderOptions) {
    let fixture = OnceCell::new();
    c.bench_function(label, move |b| {
        let fixture = fixture.get_or_init(|| {
            let fixture = ViewerRenderBenchmark::fixture_45k();
            eprintln!("{label} output_bytes={}", fixture.render(options).len());
            fixture
        });
        b.iter(|| black_box(fixture.render(black_box(options))));
    });
}

fn benchmark_raw_artifact(c: &mut Criterion) {
    let fixture = OnceCell::new();
    c.bench_function("raw-artifact", move |b| {
        let fixture = fixture.get_or_init(|| {
            let fixture = ViewerRenderBenchmark::fixture_45k();
            eprintln!("raw-artifact output_bytes={}", fixture.render_raw().len());
            fixture
        });
        b.iter(|| black_box(fixture.render_raw()));
    });
}

fn benchmark_materialized(
    c: &mut Criterion,
    fixture_label: &'static str,
    fixture_factory: fn() -> ViewerRenderBenchmark,
) {
    let shell_fixture = OnceCell::new();
    c.bench_function(&format!("materialized-shell-{fixture_label}"), move |b| {
        let fixture = shell_fixture.get_or_init(|| {
            let fixture = fixture_factory();
            let shell = fixture.render_shell(RenderOptions::DEFAULT);
            eprintln!("materialized-{fixture_label} shell_bytes={}", shell.len());
            fixture
        });
        b.iter(|| black_box(fixture.render_shell(black_box(RenderOptions::DEFAULT))));
    });

    let chunks_fixture = OnceCell::new();
    c.bench_function(&format!("materialized-chunks-{fixture_label}"), move |b| {
        let fixture = chunks_fixture.get_or_init(|| {
            let fixture = fixture_factory();
            let chunks = fixture.render_chunks(RenderOptions::DEFAULT);
            eprintln!(
                "materialized-{fixture_label} chunks={} chunk_bytes={} max_chunk_bytes={}",
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

criterion_group!(benches, render_large_viewer);
criterion_main!(benches);
