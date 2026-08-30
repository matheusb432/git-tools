use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use dx_preview::{Preview, Showcase, find, showcases};
use gtl_benchmarks::require;

const CURRENT_SHOWCASE_COUNT: usize = 8;
const CURRENT_PREVIEW_COUNT: usize = 4;
const SCALED_SHOWCASE_COUNT: usize = 128;
const SCALED_PREVIEW_COUNT: usize = 8;

fn render_empty() -> dx_preview::__private::dioxus::prelude::Element {
    dx_preview::__private::dioxus::prelude::VNode::empty()
}

static DEFAULT_PREVIEW: Preview =
    dx_preview::__private::preview("default", "Default", None, render_empty, "");
static COMPACT_PREVIEW: Preview =
    dx_preview::__private::preview("compact", "Compact", None, render_empty, "");
static INTERACTIVE_PREVIEW: Preview =
    dx_preview::__private::preview("interactive", "Interactive", None, render_empty, "");
static WIDE_PREVIEW: Preview =
    dx_preview::__private::preview("wide", "Wide", None, render_empty, "");
static REGISTERED_PREVIEWS: [&Preview; CURRENT_PREVIEW_COUNT] = [
    &DEFAULT_PREVIEW,
    &COMPACT_PREVIEW,
    &INTERACTIVE_PREVIEW,
    &WIDE_PREVIEW,
];

macro_rules! register_benchmark_showcase {
    ($constant:ident, $id:literal, $name:literal) => {
        static $constant: Showcase = dx_preview::__private::showcase(
            $id,
            $name,
            None,
            Some(&COMPACT_PREVIEW),
            &REGISTERED_PREVIEWS,
        );

        dx_preview::__private::submit! {
            &$constant
        }
    };
}

register_benchmark_showcase!(SHOWCASE_00, "showcase-00", "Showcase 00");
register_benchmark_showcase!(SHOWCASE_01, "showcase-01", "Showcase 01");
register_benchmark_showcase!(SHOWCASE_02, "showcase-02", "Showcase 02");
register_benchmark_showcase!(SHOWCASE_03, "showcase-03", "Showcase 03");
register_benchmark_showcase!(SHOWCASE_04, "showcase-04", "Showcase 04");
register_benchmark_showcase!(SHOWCASE_05, "showcase-05", "Showcase 05");
register_benchmark_showcase!(SHOWCASE_06, "showcase-06", "Showcase 06");
register_benchmark_showcase!(SHOWCASE_07, "showcase-07", "Showcase 07");

fn showcase_registry(criterion: &mut Criterion) {
    let current_registrations = registry_fixture(CURRENT_SHOWCASE_COUNT, CURRENT_PREVIEW_COUNT);
    let scaled_registrations = registry_fixture(SCALED_SHOWCASE_COUNT, SCALED_PREVIEW_COUNT);

    benchmark_registry_build(
        criterion,
        "showcase-registry/build/8-showcases-4-previews",
        current_registrations,
    );
    benchmark_registry_build(
        criterion,
        "showcase-registry/build/128-showcases-8-previews",
        scaled_registrations,
    );

    let registered = require(showcases(), "initializing benchmark showcase registrations");
    assert_eq!(registered.len(), CURRENT_SHOWCASE_COUNT);
    assert_eq!(
        require(
            find("showcase-07", "wide"),
            "checking the last benchmark showcase and preview",
        ),
        Some((&SHOWCASE_07, &WIDE_PREVIEW)),
    );

    benchmark_find(criterion, "first-showcase", "showcase-00", "default");
    benchmark_find(criterion, "last-showcase", "showcase-07", "default");
    benchmark_find(criterion, "last-preview", "showcase-00", "wide");
    benchmark_find(criterion, "missing-showcase", "showcase-99", "default");

    criterion.bench_function("showcase-registry/catalog-thumbnail", |bencher| {
        bencher.iter(|| black_box(&SHOWCASE_00).thumbnail());
    });
}

fn benchmark_registry_build(
    criterion: &mut Criterion,
    name: &'static str,
    registrations: &'static [&'static Showcase],
) {
    criterion.bench_function(name, |bencher| {
        bencher.iter(|| {
            black_box(require(
                dx_preview::__private::build_registry_for_benchmark(
                    black_box(registrations).iter().copied(),
                ),
                "building the showcase registry",
            ));
        });
    });
}

fn benchmark_find(
    criterion: &mut Criterion,
    case: &'static str,
    showcase_id: &'static str,
    preview_id: &'static str,
) {
    let name = format!("showcase-registry/find/{case}");
    criterion.bench_function(&name, |bencher| {
        bencher.iter(|| find(black_box(showcase_id), black_box(preview_id)));
    });
}

fn registry_fixture(showcase_count: usize, preview_count: usize) -> &'static [&'static Showcase] {
    let mut showcases = Vec::with_capacity(showcase_count);
    for showcase_index in (0..showcase_count).rev() {
        let previews = preview_fixture(showcase_index, preview_count);
        let showcase: &'static Showcase = Box::leak(Box::new(dx_preview::__private::showcase(
            leak_string(format!("showcase-{showcase_index:03}")),
            leak_string(format!("Showcase {showcase_index:03}")),
            None,
            previews.first().copied(),
            previews,
        )));
        showcases.push(showcase);
    }
    Box::leak(showcases.into_boxed_slice())
}

fn preview_fixture(showcase_index: usize, preview_count: usize) -> &'static [&'static Preview] {
    let previews = (0..preview_count)
        .map(|preview_index| {
            let preview: &'static Preview = Box::leak(Box::new(dx_preview::__private::preview(
                leak_string(format!("preview-{preview_index:02}")),
                leak_string(format!("Preview {showcase_index:03}-{preview_index:02}")),
                None,
                render_empty,
                "",
            )));
            preview
        })
        .collect::<Vec<_>>();
    Box::leak(previews.into_boxed_slice())
}

fn leak_string(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

criterion_group!(benches, showcase_registry);
criterion_main!(benches);
