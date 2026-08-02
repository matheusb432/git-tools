use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use gtl_application::{
    history::record_render::{self, RecordRender},
    ports::Clock,
};
use gtl_infra::app_state::SqliteAppState;

#[derive(Clone, Copy)]
struct BenchmarkClock;

impl Clock for BenchmarkClock {
    fn now_iso(&self) -> String {
        "2026-07-19T00:00:00Z".into()
    }
}

fn request() -> RecordRender {
    RecordRender {
        recipe: gtl_contracts::recipes::Recipe {
            source: gtl_contracts::recipes::RecipeSource::LocalRepo("/repos/gt".into()),
            op: gtl_contracts::recipes::RecipeOp::Diff {
                target: gtl_contracts::recipes::RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        },
        title: "git-tools · unpushed".into(),
        repo_name: "git-tools".into(),
        range_label: "origin/main..HEAD".into(),
    }
}

fn record_render(criterion: &mut Criterion) {
    let directory = tempfile::tempdir().expect("temporary app-state directory");
    let app_state = SqliteAppState::open(directory.path()).expect("open app state");
    criterion.bench_function("app-state-record-render", |bencher| {
        bencher.iter_batched(
            request,
            |request| {
                record_render::execute(black_box(request), &app_state, &BenchmarkClock)
                    .expect("record render");
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, record_render);
criterion_main!(benches);
