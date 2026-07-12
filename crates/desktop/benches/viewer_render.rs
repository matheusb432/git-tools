use criterion::{Criterion, criterion_group, criterion_main};
use desktop::benchmark_support::ViewerRenderBenchmark;
use domain::viewer::{DiffDensity, DiffLayout, RenderOptions};

fn render_large_viewer(c: &mut Criterion) {
    let fixture = ViewerRenderBenchmark::fixture_45k();

    for (label, options) in [
        (
            "unified-compact",
            RenderOptions::new(DiffLayout::Unified, DiffDensity::Compact),
        ),
        (
            "split-full",
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
        ),
    ] {
        eprintln!("{label} output_bytes={}", fixture.render(options).len());
        c.bench_function(label, |b| b.iter(|| fixture.render(options)));
    }
}

criterion_group!(benches, render_large_viewer);
criterion_main!(benches);
