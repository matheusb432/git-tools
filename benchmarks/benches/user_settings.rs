use std::{fmt::Write as _, hint::black_box, time::Duration};

use criterion::{Criterion, criterion_group, criterion_main};
use gtl_application::ports::UserSettingsReader as _;
use gtl_benchmarks::require;
use gtl_infra::user_config::TomlSettingsStore;

fn user_settings(criterion: &mut Criterion) {
    let directory = require(tempfile::tempdir(), "creating a settings directory");
    let path = directory.path().join("config.toml");
    let mut raw = String::from(include_str!("../../config/local/config.example.toml"));
    for index in 0..20 {
        require(
            writeln!(
                raw,
                "\n[[projects]]\nname = \"project-{index}\"\nexcluded_from_push_all = true\ndiff = {{ exclude = [\"md\", \"lock\"] }}"
            ),
            "appending project settings",
        );
    }
    require(std::fs::write(&path, raw), "writing settings");
    let store = TomlSettingsStore::new(Some(path.clone()));
    let expected = require(store.load(), "priming settings");
    let mut group = criterion.benchmark_group("user-settings");
    group
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(1))
        .sample_size(30);

    group.bench_function("cold", |bencher| {
        bencher.iter(|| {
            let store = TomlSettingsStore::new(Some(path.clone()));
            black_box(require(store.load(), "loading cold settings"))
        });
    });
    group.bench_function("load", |bencher| {
        bencher.iter(|| black_box(require(store.clone().load(), "loading settings")));
    });
    group.bench_function("viewer", |bencher| {
        bencher.iter(|| {
            black_box(require(
                store.clone().load_viewer_settings(),
                "loading viewer settings",
            ))
        });
    });
    group.bench_function("concurrent-128-reads", |bencher| {
        bencher.iter(|| concurrent_reads(&store));
    });
    group.finish();
    assert_eq!(require(store.load(), "checking settings"), expected);
    assert_eq!(
        require(store.load_viewer_settings(), "checking viewer settings").0,
        expected
    );
}

fn concurrent_reads(store: &TomlSettingsStore) {
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let store = store.clone();
            scope.spawn(move || read_batch(&store));
        }
    });
}

fn read_batch(store: &TomlSettingsStore) {
    for _ in 0..32 {
        black_box(require(store.clone().load(), "loading concurrent settings"));
    }
}

criterion_group!(benches, user_settings);
criterion_main!(benches);
