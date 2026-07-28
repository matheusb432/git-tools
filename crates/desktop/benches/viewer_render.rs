use std::hint::black_box;

use application::viewer::{DiffDensity, DiffLayout, RenderOptions};
use criterion::{Criterion, criterion_group, criterion_main};

#[path = "viewer_render/fixture.rs"]
mod fixture;
#[path = "fixtures/view.rs"]
mod view_fixture;

use fixture::ViewerRenderBenchmark;

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
        c.bench_function(label, |b| {
            b.iter(|| black_box(fixture.render(black_box(options))))
        });
    }

    eprintln!("raw-artifact output_bytes={}", fixture.render_raw().len());
    c.bench_function("raw-artifact", |b| {
        b.iter(|| black_box(fixture.render_raw()))
    });
}

criterion_group!(benches, render_large_viewer);
criterion_main!(benches);
