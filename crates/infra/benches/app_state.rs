use std::hint::black_box;

use application::{
    history::record_render::{self, RecordRender},
    ports::Clock,
};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use infra::app_state::SqliteAppState;

#[derive(Clone, Copy)]
struct BenchmarkClock;

impl Clock for BenchmarkClock {
    fn now_iso(&self) -> String {
        "2026-07-19T00:00:00Z".into()
    }
}

fn request(data_root: &std::path::Path) -> RecordRender {
    RecordRender {
        data_root: data_root.to_path_buf(),
        recipe_json: r#"{"kind":"diff"}"#.into(),
        title: "git-tools · unpushed".into(),
        repo_name: "git-tools".into(),
        kind: "diff".into(),
        range_label: "origin/main..HEAD".into(),
    }
}

fn record_render(criterion: &mut Criterion) {
    let directory = tempfile::tempdir().expect("temporary app-state directory");
    let app_state = SqliteAppState::open(directory.path()).expect("open app state");
    criterion.bench_function("app-state-record-render", |bencher| {
        bencher.iter_batched(
            || request(directory.path()),
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
