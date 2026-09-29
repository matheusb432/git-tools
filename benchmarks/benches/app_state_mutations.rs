use std::{hint::black_box, time::Duration};

use criterion::{
    BatchSize, BenchmarkGroup, Criterion, criterion_group, criterion_main, measurement::WallTime,
};
use gtl_application::projects::{
    catalogue::{
        create_project::{self, CreateProject},
        set_project_membership::{self, ProjectMembership, SetProjectMembership},
    },
    set_project_status::{self, SetProjectStatus},
};
use gtl_benchmarks::require;
use gtl_infra::app_state::SqliteAppState;
use gtl_models::projects::catalogue::{
    ProjectGroups, ProjectIds, ProjectMetadata, ProjectOperationMode, ProjectStatus,
};

fn project_mutations(criterion: &mut Criterion) {
    let directory = require(tempfile::tempdir(), "creating app-state directory");
    let state = require(SqliteAppState::open(directory.path()), "opening app state");
    require(
        require(state.connection_lock(), "locking app state").execute_batch(
            "INSERT INTO project_sources (source_kind, source_value)
             VALUES ('directory', '/repos/gt');
             INSERT INTO projects (id, source_id, title)
             VALUES ('GTL', last_insert_rowid(), 'git-tools');",
        ),
        "seeding the project",
    );
    let mut group = criterion.benchmark_group("app-state-mutations");
    group.sample_size(30);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    for (case, timestamp, mode) in [
        ("changed", "NULL", ProjectOperationMode::Apply),
        (
            "unchanged",
            "'2026-07-19T00:00:00.000Z'",
            ProjectOperationMode::Apply,
        ),
        ("preview", "NULL", ProjectOperationMode::Preview),
    ] {
        let request = SetProjectStatus {
            id: require("GTL".try_into(), "creating project ID"),
            status: ProjectStatus::Paused,
            mode,
        };
        let reset = format!("UPDATE projects SET paused_at = {timestamp}, unmanaged_at = NULL");
        group.bench_function(format!("pause/{case}"), |bencher| {
            bencher.iter_batched(
                || {
                    require(
                        require(state.connection_lock(), "locking app state").execute(&reset, []),
                        "resetting project status",
                    );
                },
                |()| {
                    let mut connection = require(state.connection_lock(), "locking app state");
                    black_box(require(
                        set_project_status::execute(black_box(&request), &mut connection),
                        "pausing the project",
                    ));
                },
                BatchSize::PerIteration,
            );
        });
        let request = SetProjectMembership {
            ids: require(
                ProjectIds::try_new(vec![require("GTL".try_into(), "creating project ID")]),
                "creating project IDs",
            ),
            membership: ProjectMembership::Unmanaged,
            mode,
        };
        let reset = format!("UPDATE projects SET unmanaged_at = {timestamp}");
        group.bench_function(format!("unmanage/{case}"), |bencher| {
            bencher.iter_batched(
                || {
                    require(
                        require(state.connection_lock(), "locking app state").execute(&reset, []),
                        "resetting project membership",
                    );
                },
                |()| {
                    let mut connection = require(state.connection_lock(), "locking app state");
                    black_box(require(
                        set_project_membership::execute(black_box(&request), &mut connection),
                        "unmanaging the project",
                    ));
                },
                BatchSize::PerIteration,
            );
        });
    }
    membership_batch(&mut group, &state);
    group.finish();
}

fn membership_batch(group: &mut BenchmarkGroup<'_, WallTime>, state: &SqliteAppState) {
    let mut ids = vec![require("GTL".try_into(), "creating project ID")];
    for index in 0_u8..63 {
        let id = format!(
            "{}{}",
            char::from(b'A' + index / 26),
            char::from(b'A' + index % 26)
        );
        let request = CreateProject {
            id: require(id.clone().try_into(), "creating project ID"),
            metadata: ProjectMetadata {
                title: require(id.clone().try_into(), "creating project title"),
                source: require(format!("/repos/{id}").try_into(), "creating project source"),
                git_remote: None,
                color: None,
                groups: ProjectGroups::default(),
            },
            include_in_full_export: true,
        };
        let mut connection = require(state.connection_lock(), "locking app state");
        ids.push(require(
            create_project::execute(&request, &mut connection),
            "seeding batch project",
        ));
    }
    let request = SetProjectMembership {
        ids: require(ProjectIds::try_new(ids), "creating batch IDs"),
        membership: ProjectMembership::Unmanaged,
        mode: ProjectOperationMode::Apply,
    };
    group.bench_function("unmanage/batch-64", |bencher| {
        bencher.iter_batched(
            || {
                require(
                    require(state.connection_lock(), "locking app state")
                        .execute("UPDATE projects SET unmanaged_at = NULL", []),
                    "resetting batch membership",
                );
            },
            |()| {
                let mut connection = require(state.connection_lock(), "locking app state");
                black_box(require(
                    set_project_membership::execute(black_box(&request), &mut connection),
                    "unmanaging project batch",
                ));
            },
            BatchSize::PerIteration,
        );
    });
}

criterion_group!(benches, project_mutations);
criterion_main!(benches);
