use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use dx_story::{Story, StorySet, find, story_sets};
use gtl_benchmarks::require;

const CURRENT_STORY_SET_COUNT: usize = 8;
const CURRENT_STORY_COUNT: usize = 4;
const SCALED_STORY_SET_COUNT: usize = 128;
const SCALED_STORY_COUNT: usize = 8;

fn render_empty() -> dx_story::__private::dioxus::prelude::Element {
    dx_story::__private::dioxus::prelude::VNode::empty()
}

static DEFAULT_STORY: Story =
    dx_story::__private::story("default", "Default", None, render_empty, "");
static COMPACT_STORY: Story =
    dx_story::__private::story("compact", "Compact", None, render_empty, "");
static INTERACTIVE_STORY: Story =
    dx_story::__private::story("interactive", "Interactive", None, render_empty, "");
static WIDE_STORY: Story = dx_story::__private::story("wide", "Wide", None, render_empty, "");
static REGISTERED_STORIES: [&Story; CURRENT_STORY_COUNT] = [
    &DEFAULT_STORY,
    &COMPACT_STORY,
    &INTERACTIVE_STORY,
    &WIDE_STORY,
];

macro_rules! register_benchmark_story_set {
    ($constant:ident, $id:literal, $name:literal) => {
        static $constant: StorySet = dx_story::__private::story_set(
            $id,
            $name,
            None,
            Some(&COMPACT_STORY),
            &REGISTERED_STORIES,
        );

        dx_story::__private::submit! {
            &$constant
        }
    };
}

register_benchmark_story_set!(STORY_SET_00, "story-set-00", "Story set 00");
register_benchmark_story_set!(STORY_SET_01, "story-set-01", "Story set 01");
register_benchmark_story_set!(STORY_SET_02, "story-set-02", "Story set 02");
register_benchmark_story_set!(STORY_SET_03, "story-set-03", "Story set 03");
register_benchmark_story_set!(STORY_SET_04, "story-set-04", "Story set 04");
register_benchmark_story_set!(STORY_SET_05, "story-set-05", "Story set 05");
register_benchmark_story_set!(STORY_SET_06, "story-set-06", "Story set 06");
register_benchmark_story_set!(STORY_SET_07, "story-set-07", "Story set 07");

fn story_set_registry(criterion: &mut Criterion) {
    let current_registrations = registry_fixture(CURRENT_STORY_SET_COUNT, CURRENT_STORY_COUNT);
    let scaled_registrations = registry_fixture(SCALED_STORY_SET_COUNT, SCALED_STORY_COUNT);

    benchmark_registry_build(
        criterion,
        "story-set-registry/build/8-story-sets-4-stories",
        current_registrations,
    );
    benchmark_registry_build(
        criterion,
        "story-set-registry/build/128-story-sets-8-stories",
        scaled_registrations,
    );

    let registered = require(
        story_sets(),
        "initializing benchmark story-set registrations",
    );
    assert_eq!(registered.len(), CURRENT_STORY_SET_COUNT);
    assert_eq!(
        require(
            find("story-set-07", "wide"),
            "checking the last benchmark story set and story",
        ),
        Some((&STORY_SET_07, &WIDE_STORY)),
    );

    benchmark_find(criterion, "first-story-set", "story-set-00", "default");
    benchmark_find(criterion, "last-story-set", "story-set-07", "default");
    benchmark_find(criterion, "last-story", "story-set-00", "wide");
    benchmark_find(criterion, "missing-story-set", "story-set-99", "default");

    criterion.bench_function("story-set-registry/catalog-thumbnail", |bencher| {
        bencher.iter(|| black_box(&STORY_SET_00).thumbnail());
    });
}

fn benchmark_registry_build(
    criterion: &mut Criterion,
    name: &'static str,
    registrations: &'static [&'static StorySet],
) {
    criterion.bench_function(name, |bencher| {
        bencher.iter(|| {
            black_box(require(
                dx_story::__private::build_registry_for_benchmark(
                    black_box(registrations).iter().copied(),
                ),
                "building the story-set registry",
            ));
        });
    });
}

fn benchmark_find(
    criterion: &mut Criterion,
    case: &'static str,
    story_set_id: &'static str,
    story_id: &'static str,
) {
    let name = format!("story-set-registry/find/{case}");
    criterion.bench_function(&name, |bencher| {
        bencher.iter(|| find(black_box(story_set_id), black_box(story_id)));
    });
}

fn registry_fixture(story_set_count: usize, story_count: usize) -> &'static [&'static StorySet] {
    let mut story_sets = Vec::with_capacity(story_set_count);
    for story_set_index in (0..story_set_count).rev() {
        let stories = story_fixture(story_set_index, story_count);
        let story_set: &'static StorySet = Box::leak(Box::new(dx_story::__private::story_set(
            leak_string(format!("story-set-{story_set_index:03}")),
            leak_string(format!("Story set {story_set_index:03}")),
            None,
            stories.first().copied(),
            stories,
        )));
        story_sets.push(story_set);
    }
    Box::leak(story_sets.into_boxed_slice())
}

fn story_fixture(story_set_index: usize, story_count: usize) -> &'static [&'static Story] {
    let stories = (0..story_count)
        .map(|story_index| {
            let story: &'static Story = Box::leak(Box::new(dx_story::__private::story(
                leak_string(format!("story-{story_index:02}")),
                leak_string(format!("Story {story_set_index:03}-{story_index:02}")),
                None,
                render_empty,
                "",
            )));
            story
        })
        .collect::<Vec<_>>();
    Box::leak(stories.into_boxed_slice())
}

fn leak_string(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

criterion_group!(benches, story_set_registry);
criterion_main!(benches);
