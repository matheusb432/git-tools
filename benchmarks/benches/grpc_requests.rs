use std::{hint::black_box, path::Path, process::Command};

use criterion::{Criterion, criterion_group, criterion_main};
use gtl_benchmarks::{Benchmark, BenchmarkCase, require};
use gtl_client::GtlClient;
use gtl_server::ServerHarness;
use gtl_wire::v1::{
    DiffTarget, Empty, GetRepositoryStatusRequest, PrepareDiffRequest, diff_target,
};
use prost::Message as _;

const TOKIO_WORKER_THREADS: usize = 2;

struct RepositoryFixture {
    _directory: tempfile::TempDir,
    path: String,
}

fn grpc_requests(criterion: &mut Criterion) {
    let data_root = require(tempfile::tempdir(), "creating the benchmark data root");
    let settings_path = data_root.path().join("settings.toml");
    require(
        std::fs::write(&settings_path, "[push]\nconfirm = true\n"),
        "writing the benchmark settings",
    );
    let repository = repository_fixture();
    let runtime = require(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(TOKIO_WORKER_THREADS)
            .enable_all()
            .build(),
        "building the benchmark Tokio runtime",
    );
    let (server, client) = runtime.block_on(async {
        let server = require(
            ServerHarness::start(data_root.path(), Some(settings_path)).await,
            "starting the benchmark gRPC server",
        );
        let client = require(
            GtlClient::connect(server.auth()).await,
            "connecting the benchmark gRPC client",
        );
        (server, client)
    });

    benchmark_get_push_confirmation_requirement(criterion, &runtime, &client);
    benchmark_get_repository_status(criterion, &runtime, &client, &repository.path);
    benchmark_prepare_diff(criterion, &runtime, &client, &repository.path);

    runtime.block_on(async {
        require(server.stop().await, "stopping the benchmark gRPC server");
    });
}

fn benchmark_get_push_confirmation_requirement(
    criterion: &mut Criterion,
    runtime: &tokio::runtime::Runtime,
    client: &GtlClient,
) {
    let response = runtime.block_on(async {
        require(
            client.get_push_confirmation_requirement().await,
            "warming the get-push-confirmation-requirement request",
        )
    });
    assert!(response.push_confirmation_required);
    report_output_size(
        BenchmarkCase::GrpcRequestsGetPushConfirmationRequirement,
        response.encoded_len(),
    );

    criterion.bench_function(
        BenchmarkCase::GrpcRequestsGetPushConfirmationRequirement.as_str(),
        |bencher| {
            bencher.to_async(runtime).iter(|| async {
                black_box(require(
                    client.get_push_confirmation_requirement().await,
                    "executing the get-push-confirmation-requirement request",
                ));
            });
        },
    );
}

fn benchmark_get_repository_status(
    criterion: &mut Criterion,
    runtime: &tokio::runtime::Runtime,
    client: &GtlClient,
    repository_path: &str,
) {
    let request = GetRepositoryStatusRequest {
        repository_path: repository_path.to_owned(),
    };
    let response = runtime.block_on(async {
        require(
            client.get_repository_status(request.clone()).await,
            "warming the get-repository-status request",
        )
    });
    assert_eq!(response.results.len(), 1);
    report_output_size(
        BenchmarkCase::GrpcRequestsGetRepositoryStatus,
        response.encoded_len(),
    );

    criterion.bench_function(
        BenchmarkCase::GrpcRequestsGetRepositoryStatus.as_str(),
        |bencher| {
            bencher.to_async(runtime).iter(|| async {
                black_box(require(
                    client.get_repository_status(request.clone()).await,
                    "executing the get-repository-status request",
                ));
            });
        },
    );
}

fn benchmark_prepare_diff(
    criterion: &mut Criterion,
    runtime: &tokio::runtime::Runtime,
    client: &GtlClient,
    repository_path: &str,
) {
    let request = PrepareDiffRequest {
        working_directory: repository_path.to_owned(),
        target: Some(DiffTarget {
            selection: Some(diff_target::Selection::Unpushed(Empty {})),
        }),
        name: None,
    };
    let response = runtime.block_on(async {
        require(
            client.prepare_diff(request.clone()).await,
            "warming the prepare-diff request",
        )
    });
    assert_eq!(
        response.batch.as_ref().map(|batch| batch.recipes.len()),
        Some(1)
    );
    report_output_size(
        BenchmarkCase::GrpcRequestsPrepareDiffUnpushed,
        response.encoded_len(),
    );

    criterion.bench_function(
        BenchmarkCase::GrpcRequestsPrepareDiffUnpushed.as_str(),
        |bencher| {
            bencher.to_async(runtime).iter(|| async {
                black_box(require(
                    client.prepare_diff(request.clone()).await,
                    "executing the prepare-diff request",
                ));
            });
        },
    );
}

fn report_output_size(case: BenchmarkCase, output_bytes: usize) {
    eprintln!("{case} output_bytes={output_bytes}");
}

fn repository_fixture() -> RepositoryFixture {
    let directory = require(
        tempfile::tempdir(),
        "creating the benchmark repository fixture",
    );
    let repository = directory.path().join("repository");
    let origin = directory.path().join("origin.git");
    require(
        std::fs::create_dir(&repository),
        "creating the benchmark repository",
    );
    git(&repository, &["init", "-q", "-b", "main"]);
    git(&repository, &["config", "user.name", "Benchmark"]);
    git(
        &repository,
        &["config", "user.email", "benchmark@example.invalid"],
    );
    git(&repository, &["config", "commit.gpgsign", "false"]);
    git(&repository, &["config", "core.autocrlf", "false"]);
    git(&repository, &["config", "core.hooksPath", "/dev/null"]);
    require(
        std::fs::write(repository.join("README.md"), "benchmark fixture\n"),
        "writing the benchmark repository base file",
    );
    git(&repository, &["add", "README.md"]);
    git(&repository, &["commit", "-qm", "benchmark base"]);

    let origin_path = origin.to_string_lossy().into_owned();
    git(&repository, &["init", "--bare", "-q", &origin_path]);
    git(&repository, &["remote", "add", "origin", &origin_path]);
    git(&repository, &["push", "-q", "-u", "origin", "main"]);

    require(
        std::fs::write(repository.join("unpushed.txt"), "unpushed commit\n"),
        "writing the benchmark repository unpushed file",
    );
    git(&repository, &["add", "unpushed.txt"]);
    git(&repository, &["commit", "-qm", "benchmark unpushed"]);

    RepositoryFixture {
        path: repository.to_string_lossy().into_owned(),
        _directory: directory,
    }
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = require(
        Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(arguments)
            .output(),
        "starting Git for the benchmark fixture",
    );
    if !output.status.success() {
        eprintln!(
            "benchmark setup failed while running git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        std::process::exit(1);
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(Benchmark::GrpcRequests.sample_size());
    targets = grpc_requests
}
criterion_main!(benches);
