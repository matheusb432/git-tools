use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use gtl_application::{
    history::record_render::{self, RecordRender},
    ports::Clock,
};
use gtl_benchmarks::{Benchmark, BenchmarkCase, require};
use gtl_infra::app_state::SqliteAppState;

#[derive(Clone)]
struct BenchmarkClock(gtl_models::timestamps::MachineTimestamp);

impl Clock for BenchmarkClock {
    fn now(
        &self,
    ) -> Result<gtl_models::timestamps::MachineTimestamp, gtl_models::timestamps::TimestampError>
    {
        Ok(self.0.clone())
    }
}

fn request() -> RecordRender {
    RecordRender {
        recipe: gtl_wire::recipes::Recipe {
            source: gtl_wire::recipes::RecipeSource::LocalRepo(require(
                gtl_models::paths::RepositoryRoot::try_new("/repos/gt".into()),
                "creating the benchmark repository root",
            )),
            op: gtl_wire::recipes::RecipeOp::Diff {
                target: gtl_wire::recipes::RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        },
        title: "git-tools · unpushed".into(),
        repo_name: require(
            gtl_models::paths::ProjectName::try_from("git-tools"),
            "creating the benchmark project name",
        ),
        range_label: "origin/main..HEAD".into(),
    }
}

fn record_render(criterion: &mut Criterion) {
    let directory = require(
        tempfile::tempdir(),
        "creating a temporary app-state directory",
    );
    let app_state = require(SqliteAppState::open(directory.path()), "opening app state");
    let clock = BenchmarkClock(require(
        gtl_models::timestamps::MachineTimestamp::try_from("2026-07-19T00:00:00Z"),
        "parsing the benchmark timestamp",
    ));
    criterion.bench_function(BenchmarkCase::AppStateRecordRender.as_str(), |bencher| {
        bencher.iter_batched(
            request,
            |request| {
                let mut connection = require(app_state.connection_lock(), "locking app state");
                require(
                    record_render::execute(black_box(&request), &mut connection, &clock),
                    "recording a render",
                );
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(Benchmark::AppStateRecordRender.sample_size());
    targets = record_render
}
criterion_main!(benches);
