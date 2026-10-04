use std::{
    fmt::Write as _,
    hint::black_box,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use criterion::{Criterion, criterion_group};
use gtl_application::recipes::{Recipe, RecipeBatch, RecipeOp, RecipeSource, RecipeTarget};
use gtl_benchmarks::require;
use gtl_client::GtlClient;
use gtl_local_transport::LocalEndpoint;
use gtl_models::{paths::RepositoryRoot, recipes::RecipeBatchId};
use gtl_server::ServerHarness;
use gtl_wire::{
    v1::{
        self, DiffTarget, Empty, GetRepositoryStatusRequest, GetViewerShellRequest,
        PresentDiffRequest, StreamViewerRowsRequest, diff_target,
        viewer_service_client::ViewerServiceClient,
    },
    viewer::VIEWER_ROW_MAX_ENCODED_BYTES,
};
use prost::Message as _;
use sha2::{Digest as _, Sha256};
use tonic::transport::Channel;

const TOKIO_WORKER_THREADS: usize = 2;
const GIT_ISOLATION_MARKER: &str = "GTL_GRPC_BENCHMARK_GIT_ISOLATED";
const DISABLE_VIEWER_LAUNCH_MARKER: &str = "GTL_BENCHMARK_DISABLE_VIEWER_LAUNCH";
const GET_PUSH_CONFIRMATION_REQUIREMENT_BENCHMARK_NAME: &str =
    "grpc-requests/get-push-confirmation-requirement";
const GET_REPOSITORY_STATUS_BENCHMARK_NAME: &str = "grpc-requests/get-repository-status";
const PRESENT_DIFF_UNPUSHED_BENCHMARK_NAME: &str = "grpc-requests/present-diff-unpushed";
const GET_VIEWER_SHELL_BENCHMARK_NAME: &str = "grpc-requests/get-viewer-shell";
const STREAM_VIEWER_ROWS_BENCHMARK_NAME: &str = "grpc-requests/stream-viewer-rows/2k-rust";
const STREAM_VIEWER_TEXT_ROWS_BENCHMARK_NAME: &str = "grpc-requests/stream-viewer-rows/2k-text";
const VIEWER_READY_ATTEMPTS: usize = 500;
const VIEWER_READY_RETRY_DELAY: Duration = Duration::from_millis(10);

type BenchmarkViewerClient = ViewerServiceClient<Channel>;

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
    let viewer_repository = viewer_repository_fixture("src/benchmark.rs");
    let text_repository = viewer_repository_fixture("benchmark.txt");
    let runtime = require(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(TOKIO_WORKER_THREADS)
            .enable_all()
            .build(),
        "building the benchmark Tokio runtime",
    );
    let (server, client, viewer_client) = runtime.block_on(async {
        let server = require(
            ServerHarness::start(data_root.path(), Some(settings_path)).await,
            "starting the benchmark gRPC server",
        );
        let client = require(
            GtlClient::connect(server.endpoint()).await,
            "connecting the benchmark gRPC client",
        );
        let viewer_client = connect_viewer_client(server.endpoint()).await;
        (server, client, viewer_client)
    });

    benchmark_get_push_confirmation_requirement(criterion, &runtime, &client);
    benchmark_get_repository_status(criterion, &runtime, &client, &repository.path);
    benchmark_present_diff(criterion, &runtime, &client, &repository.path);

    let identity = runtime.block_on(async {
        require(
            server.open_viewer_recipe_batch(viewer_recipe_batch(&viewer_repository.path)),
            "opening the benchmark viewer recipe",
        );
        wait_for_ready_view(&viewer_client).await
    });
    benchmark_get_viewer_shell(criterion, &runtime, &viewer_client);
    benchmark_stream_viewer_rows(
        criterion,
        &runtime,
        &viewer_client,
        identity,
        STREAM_VIEWER_ROWS_BENCHMARK_NAME,
    );
    benchmark_native_row_windows(criterion, &runtime, server.endpoint(), identity);

    let identity = runtime.block_on(async {
        require(
            server.open_viewer_recipe_batch(viewer_recipe_batch(&text_repository.path)),
            "opening the benchmark text recipe",
        );
        wait_for_ready_view(&viewer_client).await
    });
    benchmark_stream_viewer_rows(
        criterion,
        &runtime,
        &viewer_client,
        identity,
        STREAM_VIEWER_TEXT_ROWS_BENCHMARK_NAME,
    );

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
        GET_PUSH_CONFIRMATION_REQUIREMENT_BENCHMARK_NAME,
        response.encoded_len(),
    );

    criterion.bench_function(
        GET_PUSH_CONFIRMATION_REQUIREMENT_BENCHMARK_NAME,
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
    report_output_size(GET_REPOSITORY_STATUS_BENCHMARK_NAME, response.encoded_len());

    criterion.bench_function(GET_REPOSITORY_STATUS_BENCHMARK_NAME, |bencher| {
        bencher.to_async(runtime).iter(|| async {
            black_box(require(
                client.get_repository_status(request.clone()).await,
                "executing the get-repository-status request",
            ));
        });
    });
}

fn benchmark_present_diff(
    criterion: &mut Criterion,
    runtime: &tokio::runtime::Runtime,
    client: &GtlClient,
    repository_path: &str,
) {
    let request = PresentDiffRequest {
        working_directory: repository_path.to_owned(),
        target: Some(DiffTarget {
            selection: Some(diff_target::Selection::Unpushed(Empty {})),
        }),
        name: None,
    };
    let response = runtime.block_on(async {
        require(
            client.present_diff(request.clone()).await,
            "warming the present-diff request",
        )
    });
    assert!(response.presentation.is_some());
    report_output_size(PRESENT_DIFF_UNPUSHED_BENCHMARK_NAME, response.encoded_len());

    criterion.bench_function(PRESENT_DIFF_UNPUSHED_BENCHMARK_NAME, |bencher| {
        bencher.to_async(runtime).iter(|| async {
            black_box(require(
                client.present_diff(request.clone()).await,
                "executing the present-diff request",
            ));
        });
    });
}

fn benchmark_get_viewer_shell(
    criterion: &mut Criterion,
    runtime: &tokio::runtime::Runtime,
    client: &BenchmarkViewerClient,
) {
    let response = runtime.block_on(async {
        let mut client = client.clone();
        require(
            client.get_viewer_shell(GetViewerShellRequest {}).await,
            "warming the get-viewer-shell request",
        )
        .into_inner()
    });
    assert!(response.shell.is_some());
    report_output_size(GET_VIEWER_SHELL_BENCHMARK_NAME, response.encoded_len());

    criterion.bench_function(GET_VIEWER_SHELL_BENCHMARK_NAME, |bencher| {
        bencher
            .to_async(runtime)
            .iter(|| measure_get_viewer_shell(client.clone()));
    });
}

async fn measure_get_viewer_shell(mut client: BenchmarkViewerClient) {
    black_box(require(
        client.get_viewer_shell(GetViewerShellRequest {}).await,
        "executing the get-viewer-shell request",
    ));
}

fn benchmark_stream_viewer_rows(
    criterion: &mut Criterion,
    runtime: &tokio::runtime::Runtime,
    client: &BenchmarkViewerClient,
    identity: v1::ViewerViewIdentity,
    label: &str,
) {
    let request = StreamViewerRowsRequest {
        identity: Some(identity),
        file_id: None,
        row_range: None,
    };
    let (message_count, output_bytes) = runtime.block_on(consume_viewer_rows(
        client.clone(),
        request.clone(),
        "warming the stream-viewer-rows request",
    ));
    assert!(message_count > 2);
    report_output_size(label, output_bytes);
    let fingerprint = runtime.block_on(viewer_rows_fingerprint(client.clone(), request.clone()));
    eprintln!("{label} messages={message_count} sha256={fingerprint}");

    criterion.bench_function(label, |bencher| {
        bencher.to_async(runtime).iter(|| {
            consume_viewer_rows(
                client.clone(),
                request.clone(),
                "executing the stream-viewer-rows request",
            )
        });
    });
}

async fn viewer_rows_fingerprint(
    mut client: BenchmarkViewerClient,
    request: StreamViewerRowsRequest,
) -> String {
    let mut stream = require(
        client.stream_viewer_rows(request).await,
        "verify viewer rows",
    )
    .into_inner();
    let mut digest = Sha256::new();
    while let Some(message) = require(stream.message().await, "verify viewer row frame") {
        let bytes = message.encode_to_vec();
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    digest
        .finalize()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            require(write!(hex, "{byte:02x}"), "formatting the row fingerprint");
            hex
        })
}

fn benchmark_native_row_windows(
    criterion: &mut Criterion,
    runtime: &tokio::runtime::Runtime,
    endpoint: &LocalEndpoint,
    identity: v1::ViewerViewIdentity,
) {
    let client = runtime.block_on(async {
        require(
            gtl_client::ViewerClient::connect(endpoint).await,
            "connect native row client",
        )
    });
    let identity = require(
        gtl_wire::proto::viewer::decode_viewer_view_identity(identity),
        "decode native row identity",
    );
    let requests: Vec<_> = (0..2_000)
        .step_by(64)
        .map(|start| gtl_wire::viewer::StreamViewerRows {
            identity,
            file: Some(gtl_wire::viewer::ViewerDiffFileId::for_index(0)),
            row_range: Some(require(
                gtl_wire::viewer::ViewerRowRange::try_new(start, (2_000 - start).min(64)),
                "native row range",
            )),
        })
        .collect();
    runtime.block_on(consume_native_row_windows(client.clone(), &requests));
    criterion.bench_function("grpc-requests/native-row-windows/2k-rust", |bencher| {
        bencher
            .to_async(runtime)
            .iter(|| consume_native_row_windows(client.clone(), &requests));
    });
}

async fn consume_native_row_windows(
    mut client: gtl_client::ViewerClient,
    requests: &[gtl_wire::viewer::StreamViewerRows],
) {
    for request in requests {
        let mut stream = require(
            client.stream_rows(request.clone()).await,
            "start native row demand",
        );
        while let Some(frame) = require(stream.message_bytes().await, "receive native row frame") {
            black_box(frame);
        }
    }
}

async fn consume_viewer_rows(
    mut client: BenchmarkViewerClient,
    request: StreamViewerRowsRequest,
    context: &'static str,
) -> (usize, usize) {
    let mut stream = require(client.stream_viewer_rows(request).await, context).into_inner();
    let mut message_count = 0_usize;
    let mut output_bytes = 0_usize;
    loop {
        let Some(message) = require(stream.message().await, context) else {
            break;
        };
        message_count += 1;
        output_bytes += message.encoded_len();
    }
    black_box((message_count, output_bytes))
}

async fn connect_viewer_client(endpoint: &LocalEndpoint) -> BenchmarkViewerClient {
    let channel = require(
        endpoint
            .connect(Duration::from_secs(1), Duration::from_secs(30))
            .await,
        "connecting the benchmark viewer client",
    );
    ViewerServiceClient::new(channel)
        .max_decoding_message_size(VIEWER_ROW_MAX_ENCODED_BYTES + 64 * 1024)
}

async fn wait_for_ready_view(client: &BenchmarkViewerClient) -> v1::ViewerViewIdentity {
    for _ in 0..VIEWER_READY_ATTEMPTS {
        let mut client = client.clone();
        let response = require(
            client.get_viewer_shell(GetViewerShellRequest {}).await,
            "waiting for the benchmark viewer recipe",
        )
        .into_inner();
        let identity = response
            .shell
            .and_then(|shell| shell.active)
            .and_then(|active| active.state)
            .and_then(|state| match state {
                v1::viewer_active_state::State::Ready(ready) => ready.view,
                v1::viewer_active_state::State::Empty(_)
                | v1::viewer_active_state::State::Pending(_)
                | v1::viewer_active_state::State::Broken(_)
                | v1::viewer_active_state::State::Error(_) => None,
            })
            .filter(|view| view.row_source() == v1::ViewerRowSourceState::Ready)
            .and_then(|view| view.identity);
        if let Some(identity) = identity {
            return identity;
        }
        tokio::time::sleep(VIEWER_READY_RETRY_DELAY).await;
    }
    eprintln!("benchmark setup failed while waiting for the viewer recipe");
    std::process::exit(1);
}

fn viewer_recipe_batch(repository_path: &str) -> RecipeBatch {
    RecipeBatch {
        batch_id: RecipeBatchId::generate(),
        recipes: vec![Recipe {
            source: RecipeSource::LocalRepo {
                root: require(
                    RepositoryRoot::try_new(PathBuf::from(repository_path)),
                    "creating the benchmark viewer repository root",
                ),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
            },
            name: None,
        }],
    }
}

fn report_output_size(benchmark_name: &str, output_bytes: usize) {
    eprintln!("{benchmark_name} output_bytes={output_bytes}");
}

fn repository_fixture() -> RepositoryFixture {
    repository_fixture_with_change("unpushed.txt", "unpushed commit\n")
}

fn viewer_repository_fixture(path: &str) -> RepositoryFixture {
    let mut source = String::new();
    for line in 0..2_000 {
        require(
            writeln!(
                source,
                "pub fn benchmark_line_{line}() -> usize {{ {line} }}"
            ),
            "building the benchmark viewer source",
        );
    }
    repository_fixture_with_change(path, &source)
}

fn repository_fixture_with_change(relative_path: &str, contents: &str) -> RepositoryFixture {
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

    let changed_file = repository.join(relative_path);
    if let Some(parent) = changed_file.parent() {
        require(
            std::fs::create_dir_all(parent),
            "creating the benchmark change directory",
        );
    }
    require(
        std::fs::write(&changed_file, contents),
        "writing the benchmark repository unpushed file",
    );
    git(&repository, &["add", relative_path]);
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
    config = Criterion::default();
    targets = grpc_requests
}

fn main() {
    if std::env::var_os(GIT_ISOLATION_MARKER).is_some() {
        benches();
        return;
    }

    let executable = require(
        std::env::current_exe(),
        "resolving the gRPC benchmark executable",
    );
    let status = require(
        Command::new(executable)
            .args(std::env::args_os().skip(1))
            .env(GIT_ISOLATION_MARKER, "1")
            .env(DISABLE_VIEWER_LAUNCH_MARKER, "1")
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .status(),
        "starting the Git-isolated gRPC benchmark process",
    );
    std::process::exit(status.code().unwrap_or(1));
}
