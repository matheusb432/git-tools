use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use dx_book::{Story, StoryVariant, find, stories};
use gtl_benchmarks::require;

const CURRENT_STORY_COUNT: usize = 8;
const CURRENT_VARIANT_COUNT: usize = 4;
const SCALED_STORY_COUNT: usize = 128;
const SCALED_VARIANT_COUNT: usize = 8;

fn render_empty() -> dx_book::__private::dioxus::prelude::Element {
    dx_book::__private::dioxus::prelude::VNode::empty()
}

static DEFAULT_VARIANT: StoryVariant =
    dx_book::__private::story_variant("default", "Default", None, render_empty, "");
static COMPACT_VARIANT: StoryVariant =
    dx_book::__private::story_variant("compact", "Compact", None, render_empty, "");
static INTERACTIVE_VARIANT: StoryVariant =
    dx_book::__private::story_variant("interactive", "Interactive", None, render_empty, "");
static WIDE_VARIANT: StoryVariant =
    dx_book::__private::story_variant("wide", "Wide", None, render_empty, "");
static REGISTERED_VARIANTS: [&StoryVariant; CURRENT_VARIANT_COUNT] = [
    &DEFAULT_VARIANT,
    &COMPACT_VARIANT,
    &INTERACTIVE_VARIANT,
    &WIDE_VARIANT,
];

macro_rules! register_benchmark_story {
    ($constant:ident, $id:literal, $name:literal) => {
        static $constant: Story = dx_book::__private::story(
            $id,
            $name,
            None,
            Some(&COMPACT_VARIANT),
            &REGISTERED_VARIANTS,
        );

        dx_book::__private::submit! {
            &$constant
        }
    };
}

register_benchmark_story!(STORY_00, "story-00", "Story 00");
register_benchmark_story!(STORY_01, "story-01", "Story 01");
register_benchmark_story!(STORY_02, "story-02", "Story 02");
register_benchmark_story!(STORY_03, "story-03", "Story 03");
register_benchmark_story!(STORY_04, "story-04", "Story 04");
register_benchmark_story!(STORY_05, "story-05", "Story 05");
register_benchmark_story!(STORY_06, "story-06", "Story 06");
register_benchmark_story!(STORY_07, "story-07", "Story 07");

fn storybook_registry(criterion: &mut Criterion) {
    let current_registrations = registry_fixture(CURRENT_STORY_COUNT, CURRENT_VARIANT_COUNT);
    let scaled_registrations = registry_fixture(SCALED_STORY_COUNT, SCALED_VARIANT_COUNT);

    benchmark_registry_build(
        criterion,
        "storybook-registry/build/8-stories-4-variants",
        current_registrations,
    );
    benchmark_registry_build(
        criterion,
        "storybook-registry/build/128-stories-8-variants",
        scaled_registrations,
    );

    let registered = require(stories(), "initializing benchmark story registrations");
    assert_eq!(registered.len(), CURRENT_STORY_COUNT);
    assert_eq!(
        require(
            find("story-07", "wide"),
            "checking the last benchmark story and variant",
        ),
        Some((&STORY_07, &WIDE_VARIANT)),
    );

    benchmark_find(criterion, "first-story", "story-00", "default");
    benchmark_find(criterion, "last-story", "story-07", "default");
    benchmark_find(criterion, "last-variant", "story-00", "wide");
    benchmark_find(criterion, "missing-story", "story-99", "default");

    criterion.bench_function("storybook-registry/catalog-preview", |bencher| {
        bencher.iter(|| black_box(&STORY_00).catalog_preview());
    });
}

fn benchmark_registry_build(
    criterion: &mut Criterion,
    name: &'static str,
    registrations: &'static [&'static Story],
) {
    criterion.bench_function(name, |bencher| {
        bencher.iter(|| {
            black_box(require(
                dx_book::__private::build_registry_for_benchmark(
                    black_box(registrations).iter().copied(),
                ),
                "building the story registry",
            ));
        });
    });
}

fn benchmark_find(
    criterion: &mut Criterion,
    case: &'static str,
    story_id: &'static str,
    variant_id: &'static str,
) {
    let name = format!("storybook-registry/find/{case}");
    criterion.bench_function(&name, |bencher| {
        bencher.iter(|| find(black_box(story_id), black_box(variant_id)));
    });
}

fn registry_fixture(story_count: usize, variant_count: usize) -> &'static [&'static Story] {
    let mut stories = Vec::with_capacity(story_count);
    for story_index in (0..story_count).rev() {
        let variants = variant_fixture(story_index, variant_count);
        let story: &'static Story = Box::leak(Box::new(dx_book::__private::story(
            leak_string(format!("story-{story_index:03}")),
            leak_string(format!("Story {story_index:03}")),
            None,
            variants.first().copied(),
            variants,
        )));
        stories.push(story);
    }
    Box::leak(stories.into_boxed_slice())
}

fn variant_fixture(story_index: usize, variant_count: usize) -> &'static [&'static StoryVariant] {
    let variants = (0..variant_count)
        .map(|variant_index| {
            let variant: &'static StoryVariant =
                Box::leak(Box::new(dx_book::__private::story_variant(
                    leak_string(format!("variant-{variant_index:02}")),
                    leak_string(format!("Variant {story_index:03}-{variant_index:02}")),
                    None,
                    render_empty,
                    "",
                )));
            variant
        })
        .collect::<Vec<_>>();
    Box::leak(variants.into_boxed_slice())
}

fn leak_string(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

criterion_group!(benches, storybook_registry);
criterion_main!(benches);
