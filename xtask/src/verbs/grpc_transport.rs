//! Bounded black-box measurement of the production gRPC transport.

use std::{
    fs::{self, File},
    io::Read as _,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use gtl_benchmarks::grpc_transport::{
    GrpcTransportComparison, GrpcTransportReport, GrpcTransportSummary, LaunchMetricDelta,
    LaunchMetricSummary, MetricDelta, compare_reports, summarize_report, validate_report,
};

use super::{cargo_target_directory, repository_root};
use crate::{process, task::Step};

const GHZ_VERSION: &str = "v0.121.0";
const MEMORY_SWAP_BYTES_MAX: u64 = 0;
const CARGO_JOBS_MAX: usize = 1;
const BENCHMARK_LAUNCHES: usize = 3;
const SMOKE_LAUNCHES: usize = 1;
const RSS_SAMPLE_INTERVAL_MICROSECONDS: u64 = 1_000;
// ghz 0.121.0 converts `max-duration` into duration mode, so the supervisor owns this deadline.
const BENCHMARK_GHZ_WALL_TIMEOUT_SECONDS: u64 = 600;
const SMOKE_GHZ_WALL_TIMEOUT_SECONDS: u64 = 30;
const GIT_OUTPUT_BYTES_MAX: usize = 64 * 1_024;
const TOOL_OUTPUT_BYTES_MAX: usize = 64 * 1_024;
const REPORT_BYTES_MAX: u64 = 16 * 1_024 * 1_024;
const BENCHMARK_CONFIG_RELATIVE_PATH: &str =
    "benchmarks/grpc/get_push_confirmation_requirement.toml";
const SMOKE_CONFIG_RELATIVE_PATH: &str =
    "benchmarks/grpc/get_push_confirmation_requirement_smoke.toml";
const BENCHMARK_BASELINE_RELATIVE_PATH: &str = ".artifacts/benchmarks/grpc-transport/baseline.json";
const BENCHMARK_CURRENT_RELATIVE_PATH: &str = ".artifacts/benchmarks/grpc-transport/current.json";

const BENCHMARK_BOUNDS: WorkerBounds = WorkerBounds {
    cpu_quota_percent: 200,
    memory_bytes_max: 2 * 1_024 * 1_024 * 1_024,
    process_niceness: 10,
    tasks_max: 128,
    termination_grace_seconds: 15,
    wall_time_minutes: 30,
};
const SMOKE_BOUNDS: WorkerBounds = WorkerBounds {
    wall_time_minutes: 2,
    ..BENCHMARK_BOUNDS
};

#[derive(Clone, Copy, Debug)]
struct WorkerBounds {
    cpu_quota_percent: usize,
    memory_bytes_max: u64,
    process_niceness: usize,
    tasks_max: usize,
    termination_grace_seconds: u64,
    wall_time_minutes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DriverWorkload {
    Benchmark,
    Smoke,
}

impl DriverWorkload {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Benchmark => "benchmark",
            Self::Smoke => "smoke",
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct GrpcTransportBenchmarkArguments {
    /// Compare the current result, then replace the local baseline after a successful run.
    #[arg(long)]
    pub(crate) update: bool,
}

pub(crate) fn run_benchmark(arguments: &GrpcTransportBenchmarkArguments) -> Result<()> {
    require_linux_systemd()?;
    let root = repository_root();
    let baseline_path = root.join(BENCHMARK_BASELINE_RELATIVE_PATH);
    let current_path = root.join(BENCHMARK_CURRENT_RELATIVE_PATH);
    let baseline = load_baseline(&baseline_path, arguments.update)?;
    ensure_clean_repository(&root)?;
    let ghz = locate_ghz(&root)?;
    build_release_binaries(&root)?;
    let target = cargo_target_directory(&root)?;
    let source_commit = source_commit(&root)?;
    let invocation = if arguments.update {
        "just bench-grpc --update"
    } else {
        "just bench-grpc"
    };
    process::run_step(&bounded_driver_step(&DriverInputs {
        root: &root,
        driver: &target.join("release/grpc-transport-driver"),
        server: &target.join("release/gtl-server"),
        ghz: &ghz,
        ghz_config: &root.join(BENCHMARK_CONFIG_RELATIVE_PATH),
        report: &current_path,
        launches: BENCHMARK_LAUNCHES,
        workload: DriverWorkload::Benchmark,
        ghz_wall_timeout_seconds: BENCHMARK_GHZ_WALL_TIMEOUT_SECONDS,
        source_commit: &source_commit,
        invocation,
        bounds: BENCHMARK_BOUNDS,
    }))?;

    let current = read_report(&current_path)?;
    let summary = summarize_report(&current).context("validate current gRPC transport report")?;
    println!("gRPC transport report: {}", current_path.display());
    print_summary(&summary, current.protocol.measured_request_count);
    if let Some(baseline) = baseline {
        match compare_reports(&baseline, &current) {
            Ok(comparison) => print_comparison(&comparison),
            Err(error) if arguments.update => {
                eprintln!("existing gRPC transport baseline is not comparable: {error}");
            }
            Err(error) => return Err(error.into()),
        }
    }
    if arguments.update {
        replace_file_atomically(&current_path, &baseline_path, REPORT_BYTES_MAX)?;
        println!("gRPC transport baseline: {}", baseline_path.display());
    }
    Ok(())
}

pub(crate) fn run_smoke() -> Result<()> {
    require_linux_systemd()?;
    let root = repository_root();
    let ghz = locate_ghz(&root)?;
    build_release_binaries(&root)?;
    let target = cargo_target_directory(&root)?;
    let source_commit = source_commit(&root)?;
    let report_directory = tempfile::tempdir().context("create gRPC transport smoke directory")?;
    let report_path = report_directory.path().join("report.json");
    process::run_step(&bounded_driver_step(&DriverInputs {
        root: &root,
        driver: &target.join("release/grpc-transport-driver"),
        server: &target.join("release/gtl-server"),
        ghz: &ghz,
        ghz_config: &root.join(SMOKE_CONFIG_RELATIVE_PATH),
        report: &report_path,
        launches: SMOKE_LAUNCHES,
        workload: DriverWorkload::Smoke,
        ghz_wall_timeout_seconds: SMOKE_GHZ_WALL_TIMEOUT_SECONDS,
        source_commit: &source_commit,
        invocation: "just bench-grpc-smoke",
        bounds: SMOKE_BOUNDS,
    }))?;

    let report = read_report(&report_path)?;
    validate_report(&report).context("validate gRPC transport smoke report")?;
    println!(
        "gRPC transport smoke passed ({}; {} measured requests after {} warmup requests)",
        report.transport,
        report.protocol.measured_request_count,
        report.protocol.warmup_request_count
    );
    Ok(())
}

struct DriverInputs<'input> {
    root: &'input Path,
    driver: &'input Path,
    server: &'input Path,
    ghz: &'input Path,
    ghz_config: &'input Path,
    report: &'input Path,
    launches: usize,
    workload: DriverWorkload,
    ghz_wall_timeout_seconds: u64,
    source_commit: &'input str,
    invocation: &'input str,
    bounds: WorkerBounds,
}

fn build_release_binaries(root: &Path) -> Result<()> {
    process::run_step(
        &Step::new(
            "release gRPC transport benchmark binaries",
            "cargo",
            [
                "build",
                "--locked",
                "--release",
                "-p",
                "gtl-benchmarks",
                "--bin",
                "grpc-transport-driver",
                "-p",
                "gtl-server",
                "--bin",
                "gtl-server",
            ],
        )
        .with_environment("CARGO_BUILD_JOBS", CARGO_JOBS_MAX.to_string())
        .with_current_directory(root),
    )
}

fn bounded_driver_step(inputs: &DriverInputs<'_>) -> Step {
    let bounds = inputs.bounds;
    Step::new(
        "bounded production gRPC transport measurement",
        "/usr/bin/systemd-run",
        [
            "--user".to_owned(),
            "--scope".to_owned(),
            "--quiet".to_owned(),
            "--collect".to_owned(),
            format!("--property=CPUQuota={}%", bounds.cpu_quota_percent),
            format!("--property=MemoryMax={}", bounds.memory_bytes_max),
            format!("--property=MemorySwapMax={MEMORY_SWAP_BYTES_MAX}"),
            format!("--property=TasksMax={}", bounds.tasks_max),
            "/usr/bin/nice".to_owned(),
            "-n".to_owned(),
            bounds.process_niceness.to_string(),
            "/usr/bin/timeout".to_owned(),
            "--signal=TERM".to_owned(),
            format!("--kill-after={}s", bounds.termination_grace_seconds),
            format!("{}m", bounds.wall_time_minutes),
            inputs.driver.to_string_lossy().into_owned(),
        ],
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_REPORT_PATH",
        inputs.report.to_string_lossy(),
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_SERVER_BINARY",
        inputs.server.to_string_lossy(),
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_GHZ_BINARY",
        inputs.ghz.to_string_lossy(),
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_GHZ_CONFIG",
        inputs.ghz_config.to_string_lossy(),
    )
    .with_environment("GTL_GRPC_TRANSPORT_GHZ_VERSION", GHZ_VERSION)
    .with_environment(
        "GTL_GRPC_TRANSPORT_REPOSITORY_ROOT",
        inputs.root.to_string_lossy(),
    )
    .with_environment("GTL_GRPC_TRANSPORT_LAUNCHES", inputs.launches.to_string())
    .with_environment("GTL_GRPC_TRANSPORT_WORKLOAD", inputs.workload.as_str())
    .with_environment("GTL_GRPC_TRANSPORT_SOURCE_COMMIT", inputs.source_commit)
    .with_environment("GTL_GRPC_TRANSPORT_INVOCATION", inputs.invocation)
    .with_environment(
        "GTL_GRPC_TRANSPORT_CPU_QUOTA_PERCENT",
        bounds.cpu_quota_percent.to_string(),
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_MEMORY_MAX_BYTES",
        bounds.memory_bytes_max.to_string(),
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_MEMORY_SWAP_MAX_BYTES",
        MEMORY_SWAP_BYTES_MAX.to_string(),
    )
    .with_environment("GTL_GRPC_TRANSPORT_TASKS_MAX", bounds.tasks_max.to_string())
    .with_environment(
        "GTL_GRPC_TRANSPORT_PROCESS_NICENESS",
        bounds.process_niceness.to_string(),
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_CARGO_JOBS_MAX",
        CARGO_JOBS_MAX.to_string(),
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_WALL_TIME_MINUTES",
        bounds.wall_time_minutes.to_string(),
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_TERMINATION_GRACE_SECONDS",
        bounds.termination_grace_seconds.to_string(),
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_RSS_SAMPLE_INTERVAL_MICROSECONDS",
        RSS_SAMPLE_INTERVAL_MICROSECONDS.to_string(),
    )
    .with_environment(
        "GTL_GRPC_TRANSPORT_GHZ_WALL_TIMEOUT_SECONDS",
        inputs.ghz_wall_timeout_seconds.to_string(),
    )
    .with_current_directory(inputs.root)
}

fn require_linux_systemd() -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("bounded gRPC transport benchmark requires Linux systemd user scopes");
    }
    Ok(())
}

fn locate_ghz(root: &Path) -> Result<PathBuf> {
    let output = Command::new("mise")
        .args(["which", "ghz"])
        .current_dir(root)
        .output()
        .context("resolve the Mise-managed ghz binary")?;
    ensure!(
        output.stdout.len() <= TOOL_OUTPUT_BYTES_MAX
            && output.stderr.len() <= TOOL_OUTPUT_BYTES_MAX,
        "mise which ghz output exceeded {TOOL_OUTPUT_BYTES_MAX} bytes"
    );
    ensure!(
        output.status.success(),
        "ghz {GHZ_VERSION} is not provisioned by Mise: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let path = String::from_utf8(output.stdout)
        .context("mise which ghz output is not UTF-8")?
        .trim()
        .to_owned();
    ensure!(!path.is_empty(), "mise which ghz returned an empty path");
    let path = PathBuf::from(path);
    ensure!(
        path.is_file(),
        "Mise ghz path is not a file: {}",
        path.display()
    );
    Ok(path)
}

fn ensure_clean_repository(root: &Path) -> Result<()> {
    let status = git_output(root, &["status", "--porcelain=v1", "--untracked-files=all"])?;
    ensure!(
        status.is_empty(),
        "gRPC transport measurement requires an exact committed source tree; dirty paths:\n{status}"
    );
    Ok(())
}

fn source_commit(root: &Path) -> Result<String> {
    let commit = git_output(root, &["rev-parse", "HEAD"])?;
    ensure!(
        commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Git returned an invalid source commit: {commit}"
    );
    Ok(commit)
}

fn git_output(root: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .with_context(|| format!("run Git {}", arguments.join(" ")))?;
    ensure!(
        output.stdout.len() <= GIT_OUTPUT_BYTES_MAX && output.stderr.len() <= GIT_OUTPUT_BYTES_MAX,
        "Git {} output exceeded {GIT_OUTPUT_BYTES_MAX} bytes",
        arguments.join(" ")
    );
    ensure!(
        output.status.success(),
        "Git {} failed with exit {}: {}{}",
        arguments.join(" "),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .context("Git output was not UTF-8")
        .map(|value| value.trim().to_owned())
}

fn load_baseline(path: &Path, update: bool) -> Result<Option<GrpcTransportReport>> {
    if !path.is_file() {
        ensure!(
            update,
            "gRPC transport baseline is missing at {}; run `just bench-grpc --update` first",
            path.display()
        );
        return Ok(None);
    }
    match read_report(path) {
        Ok(report) => Ok(Some(report)),
        Err(error) if update => {
            eprintln!(
                "existing gRPC transport baseline cannot be read and will be replaced: {error:#}"
            );
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn read_report(path: &Path) -> Result<GrpcTransportReport> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("read gRPC transport report metadata {}", path.display()))?;
    ensure!(
        metadata.is_file(),
        "gRPC transport report is not a file: {}",
        path.display()
    );
    ensure!(
        metadata.len() <= REPORT_BYTES_MAX,
        "gRPC transport report {} exceeds {REPORT_BYTES_MAX} bytes",
        path.display()
    );
    let file = File::open(path)
        .with_context(|| format!("open gRPC transport report {}", path.display()))?;
    serde_json::from_reader(std::io::BufReader::new(file))
        .with_context(|| format!("decode gRPC transport report {}", path.display()))
}

fn replace_file_atomically(source: &Path, destination: &Path, limit: u64) -> Result<()> {
    let source_file =
        File::open(source).with_context(|| format!("open source evidence {}", source.display()))?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("evidence destination has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create evidence directory {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary evidence in {}", parent.display()))?;
    let copied = std::io::copy(&mut source_file.take(limit + 1), &mut temporary)
        .context("copy evidence into temporary file")?;
    ensure!(copied <= limit, "evidence exceeds {limit} bytes");
    temporary.as_file().sync_all().context("sync evidence")?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)
        .with_context(|| format!("publish evidence {}", destination.display()))?;
    Ok(())
}

fn print_summary(summary: &GrpcTransportSummary, measured_requests_per_launch: u64) {
    println!(
        "gRPC transport current ({}; {measured_requests_per_launch} measured requests per launch):",
        summary.transport
    );
    println!(
        "  pooled latency p50: {:.2} us",
        summary.latency_p50_nanoseconds / 1_000.0
    );
    println!(
        "  pooled latency p95: {:.2} us",
        summary.latency_p95_nanoseconds / 1_000.0
    );
    println!(
        "  pooled latency p99: {:.2} us",
        summary.latency_p99_nanoseconds / 1_000.0
    );
    print_summary_metric("requests/s", summary.requests_per_second, 1.0);
    print_summary_metric(
        "server CPU us/issued request",
        summary.server_cpu_time_per_request_nanoseconds,
        1_000.0,
    );
    print_summary_metric(
        "peak server + ghz RSS MiB",
        summary.peak_server_and_client_rss_bytes,
        1_048_576.0,
    );
}

fn print_comparison(comparison: &GrpcTransportComparison) {
    println!(
        "gRPC transport baseline comparison ({} -> {}; launch metrics are median [min, max]):",
        comparison.baseline_transport, comparison.current_transport
    );
    print_delta_metric(
        "pooled latency p50 us",
        comparison.latency_p50_nanoseconds,
        1_000.0,
    );
    print_delta_metric(
        "pooled latency p95 us",
        comparison.latency_p95_nanoseconds,
        1_000.0,
    );
    print_delta_metric(
        "pooled latency p99 us",
        comparison.latency_p99_nanoseconds,
        1_000.0,
    );
    print_launch_delta("requests/s", comparison.requests_per_second, 1.0);
    print_launch_delta(
        "server CPU us/issued request",
        comparison.server_cpu_time_per_request_nanoseconds,
        1_000.0,
    );
    print_launch_delta(
        "peak server + ghz RSS MiB",
        comparison.peak_server_and_client_rss_bytes,
        1_048_576.0,
    );
}

fn print_summary_metric(label: &str, metric: LaunchMetricSummary, divisor: f64) {
    println!(
        "  {label}: {:.2} [{:.2}, {:.2}]",
        metric.median / divisor,
        metric.minimum / divisor,
        metric.maximum / divisor
    );
}

fn print_delta_metric(label: &str, metric: MetricDelta, divisor: f64) {
    let relative = format_relative(metric.relative_change_percent);
    println!(
        "  {label}: {:.2} -> {:.2} ({relative})",
        metric.baseline / divisor,
        metric.current / divisor
    );
}

fn print_launch_delta(label: &str, metric: LaunchMetricDelta, divisor: f64) {
    let relative = format_relative(metric.relative_change_percent);
    println!(
        "  {label}: {:.2} [{:.2}, {:.2}] -> {:.2} [{:.2}, {:.2}] ({relative})",
        metric.baseline_median / divisor,
        metric.baseline_minimum / divisor,
        metric.baseline_maximum / divisor,
        metric.current_median / divisor,
        metric.current_minimum / divisor,
        metric.current_maximum / divisor
    );
}

fn format_relative(relative: Option<f64>) -> String {
    relative.map_or_else(|| "n/a".to_owned(), |value| format!("{value:+.2}%"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_worker_records_runner_and_measurement_contracts() {
        let step = test_driver_step(
            DriverWorkload::Benchmark,
            BENCHMARK_LAUNCHES,
            BENCHMARK_GHZ_WALL_TIMEOUT_SECONDS,
            BENCHMARK_BOUNDS,
        );

        assert_eq!(step.program(), "/usr/bin/systemd-run");
        assert!(
            step.arguments()
                .contains(&"--property=CPUQuota=200%".to_owned())
        );
        assert!(
            step.arguments()
                .contains(&"--property=MemoryMax=2147483648".to_owned())
        );
        assert!(
            step.arguments()
                .contains(&"--property=MemorySwapMax=0".to_owned())
        );
        assert!(
            step.arguments()
                .contains(&"/repo/target/release/grpc-transport-driver".to_owned())
        );
        assert_eq!(
            environment_value(&step, "GTL_GRPC_TRANSPORT_GHZ_BINARY"),
            Some("/mise/ghz")
        );
        assert_eq!(
            environment_value(&step, "GTL_GRPC_TRANSPORT_LAUNCHES"),
            Some("3")
        );
        assert_eq!(
            environment_value(&step, "GTL_GRPC_TRANSPORT_WORKLOAD"),
            Some("benchmark")
        );
        assert_eq!(
            environment_value(&step, "GTL_GRPC_TRANSPORT_RSS_SAMPLE_INTERVAL_MICROSECONDS"),
            Some("1000")
        );
        assert_eq!(
            environment_value(&step, "GTL_GRPC_TRANSPORT_GHZ_WALL_TIMEOUT_SECONDS"),
            Some("600")
        );
    }

    #[test]
    fn smoke_worker_uses_the_short_non_baseline_protocol() {
        let step = test_driver_step(
            DriverWorkload::Smoke,
            SMOKE_LAUNCHES,
            SMOKE_GHZ_WALL_TIMEOUT_SECONDS,
            SMOKE_BOUNDS,
        );

        assert!(step.arguments().contains(&"2m".to_owned()));
        assert_eq!(
            environment_value(&step, "GTL_GRPC_TRANSPORT_LAUNCHES"),
            Some("1")
        );
        assert_eq!(
            environment_value(&step, "GTL_GRPC_TRANSPORT_WORKLOAD"),
            Some("smoke")
        );
        assert_eq!(
            environment_value(&step, "GTL_GRPC_TRANSPORT_GHZ_WALL_TIMEOUT_SECONDS"),
            Some("30")
        );
    }

    #[test]
    fn atomic_replacement_copies_only_the_selected_current_report() {
        let directory = tempfile::tempdir().unwrap();
        let current = directory.path().join("current.json");
        let baseline = directory.path().join("nested/baseline.json");
        fs::write(&current, b"current").unwrap();

        replace_file_atomically(&current, &baseline, 32).unwrap();

        assert_eq!(fs::read(&baseline).unwrap(), b"current");
        assert_eq!(fs::read(&current).unwrap(), b"current");
    }

    fn test_driver_step(
        workload: DriverWorkload,
        launches: usize,
        ghz_wall_timeout_seconds: u64,
        bounds: WorkerBounds,
    ) -> Step {
        bounded_driver_step(&DriverInputs {
            root: Path::new("/repo"),
            driver: Path::new("/repo/target/release/grpc-transport-driver"),
            server: Path::new("/repo/target/release/gtl-server"),
            ghz: Path::new("/mise/ghz"),
            ghz_config: Path::new("/repo/benchmarks/grpc/request.toml"),
            report: Path::new("/repo/current.json"),
            launches,
            workload,
            ghz_wall_timeout_seconds,
            source_commit: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            invocation: "just bench-grpc",
            bounds,
        })
    }

    fn environment_value<'step>(step: &'step Step, name: &str) -> Option<&'step str> {
        step.environment()
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}
