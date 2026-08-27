//! Bounded production measurement for server-owned syntax highlighting.

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    path::Path,
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use gtl_benchmarks::server_highlighting::{
    MetricDelta, ServerHighlightingComparison, ServerHighlightingReport, WorkloadComparison,
    compare_reports,
};

use super::{cargo_target_directory, repository_root};
use crate::{process, task::Step};

const MEMORY_SWAP_BYTES_MAX: u64 = 0;
const CARGO_JOBS_MAX: usize = 1;
const GIT_OUTPUT_BYTES_MAX: usize = 64 * 1_024;
const COMMAND_OUTPUT_BYTES_MAX: usize = 16 * 1_024 * 1_024;
const BENCHMARK_LAUNCHES: usize = 3;
const RSS_SAMPLE_INTERVAL_MICROSECONDS: u64 = 1_000;
const BENCHMARK_BASELINE_RELATIVE_PATH: &str =
    ".artifacts/benchmarks/server-highlighting/baseline.json";
const BENCHMARK_CURRENT_RELATIVE_PATH: &str =
    ".artifacts/benchmarks/server-highlighting/current.json";
const MASSIF_OUTPUT_RELATIVE_PATH: &str = ".artifacts/benchmarks/server-highlighting/massif.out";
const MASSIF_REPORT_RELATIVE_PATH: &str = ".artifacts/benchmarks/server-highlighting/massif.txt";
const MASSIF_DRIVER_REPORT_RELATIVE_PATH: &str =
    ".artifacts/benchmarks/server-highlighting/massif-driver.json";
const MASSIF_REPORT_THRESHOLD_PERCENT: usize = 5;
const REPORT_BYTES_MAX: u64 = 16 * 1_024 * 1_024;

const BENCHMARK_BOUNDS: WorkerBounds = WorkerBounds {
    cpu_quota_percent: 200,
    memory_bytes_max: 4 * 1_024 * 1_024 * 1_024,
    process_niceness: 10,
    tasks_max: 256,
    termination_grace_seconds: 15,
    wall_time_minutes: 20,
};

const PROFILE_BOUNDS: WorkerBounds = WorkerBounds {
    cpu_quota_percent: 200,
    memory_bytes_max: 8 * 1_024 * 1_024 * 1_024,
    process_niceness: 10,
    tasks_max: 256,
    termination_grace_seconds: 30,
    wall_time_minutes: 30,
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

#[derive(Args, Debug)]
pub(crate) struct ServerHighlightingBenchmarkArguments {
    /// Compare the current result, then replace the local baseline after a successful run.
    #[arg(long)]
    pub(crate) update: bool,
}

pub(crate) fn run_benchmark(arguments: &ServerHighlightingBenchmarkArguments) -> Result<()> {
    require_linux_systemd("server highlighting benchmark")?;
    let root = repository_root();
    let baseline_path = root.join(BENCHMARK_BASELINE_RELATIVE_PATH);
    let current_path = root.join(BENCHMARK_CURRENT_RELATIVE_PATH);
    let baseline = load_baseline(&baseline_path, arguments.update)?;
    ensure_clean_repository(&root)?;
    build_release_driver(&root)?;
    build_server(&root, "release")?;
    let target = cargo_target_directory(&root)?;
    let source_commit = source_commit(&root)?;
    let invocation = if arguments.update {
        "just bench-highlight --update"
    } else {
        "just bench-highlight"
    };
    process::run_step(&bounded_driver_step(DriverInputs {
        root: &root,
        driver: &target.join("release/server-highlighting-driver"),
        server: &target.join("release/gtl-server"),
        report: &current_path,
        launches: BENCHMARK_LAUNCHES,
        source_commit: &source_commit,
        invocation,
        bounds: BENCHMARK_BOUNDS,
        massif: None,
    }))?;

    let current = read_report(&current_path)?;
    if let Some(baseline) = baseline {
        match compare_reports(&baseline, &current) {
            Ok(comparison) => print_comparison(&comparison),
            Err(error) if arguments.update => {
                eprintln!("existing server highlighting baseline is not comparable: {error}");
            }
            Err(error) => return Err(error.into()),
        }
    }
    if arguments.update {
        replace_file_atomically(&current_path, &baseline_path, REPORT_BYTES_MAX)?;
        println!("server highlighting baseline: {}", baseline_path.display());
    }
    Ok(())
}

pub(crate) fn run_profile() -> Result<()> {
    require_linux_systemd("server highlighting allocation profile")?;
    let root = repository_root();
    ensure_clean_repository(&root)?;
    let valgrind = required_executable("/usr/bin/valgrind")?;
    let ms_print = required_executable("/usr/bin/ms_print")?;
    build_release_driver(&root)?;
    build_server(&root, "profiling")?;
    let target = cargo_target_directory(&root)?;
    let source_commit = source_commit(&root)?;
    let massif_output = root.join(MASSIF_OUTPUT_RELATIVE_PATH);
    let massif_report = root.join(MASSIF_REPORT_RELATIVE_PATH);
    process::run_step(&bounded_driver_step(DriverInputs {
        root: &root,
        driver: &target.join("release/server-highlighting-driver"),
        server: &target.join("profiling/gtl-server"),
        report: &root.join(MASSIF_DRIVER_REPORT_RELATIVE_PATH),
        launches: 1,
        source_commit: &source_commit,
        invocation: "just profile-highlight",
        bounds: PROFILE_BOUNDS,
        massif: Some(MassifInputs {
            valgrind,
            output: &massif_output,
        }),
    }))?;
    let output = Command::new(ms_print)
        .arg(format!("--threshold={MASSIF_REPORT_THRESHOLD_PERCENT}"))
        .arg(&massif_output)
        .output()
        .context("run ms_print for server highlighting profile")?;
    ensure!(
        output.stdout.len() <= COMMAND_OUTPUT_BYTES_MAX
            && output.stderr.len() <= COMMAND_OUTPUT_BYTES_MAX,
        "ms_print output exceeded {COMMAND_OUTPUT_BYTES_MAX} bytes"
    );
    ensure!(
        output.status.success(),
        "ms_print failed with exit {}: {}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    write_bytes_atomically(
        &massif_report,
        &output.stdout,
        COMMAND_OUTPUT_BYTES_MAX as u64,
    )?;
    println!(
        "server highlighting Massif report: {}",
        massif_report.display()
    );
    Ok(())
}

struct DriverInputs<'a> {
    root: &'a Path,
    driver: &'a Path,
    server: &'a Path,
    report: &'a Path,
    launches: usize,
    source_commit: &'a str,
    invocation: &'a str,
    bounds: WorkerBounds,
    massif: Option<MassifInputs<'a>>,
}

struct MassifInputs<'a> {
    valgrind: &'a Path,
    output: &'a Path,
}

fn build_release_driver(root: &Path) -> Result<()> {
    process::run_step(
        &Step::new(
            "release server highlighting driver",
            "cargo",
            [
                "build",
                "--locked",
                "--release",
                "-p",
                "gtl-benchmarks",
                "--bin",
                "server-highlighting-driver",
            ],
        )
        .with_environment("CARGO_BUILD_JOBS", CARGO_JOBS_MAX.to_string())
        .with_current_directory(root),
    )
}

fn build_server(root: &Path, profile: &str) -> Result<()> {
    let profile_arguments = if profile == "release" {
        vec!["--release".to_owned()]
    } else {
        vec!["--profile".to_owned(), profile.to_owned()]
    };
    process::run_step(
        &Step::new(
            format!("{profile} gtl-server"),
            "cargo",
            [
                "build",
                "--locked",
                "-p",
                "gtl-server",
                "--bin",
                "gtl-server",
            ],
        )
        .with_arguments(profile_arguments)
        .with_environment("CARGO_BUILD_JOBS", CARGO_JOBS_MAX.to_string())
        .with_current_directory(root),
    )
}

fn bounded_driver_step(inputs: DriverInputs<'_>) -> Step {
    let bounds = inputs.bounds;
    let mut step = Step::new(
        "bounded production server highlighting measurement",
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
        "GTL_SERVER_HIGHLIGHTING_REPORT_PATH",
        inputs.report.to_string_lossy(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_SERVER_BINARY",
        inputs.server.to_string_lossy(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_LAUNCHES",
        inputs.launches.to_string(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_SOURCE_COMMIT",
        inputs.source_commit,
    )
    .with_environment("GTL_SERVER_HIGHLIGHTING_INVOCATION", inputs.invocation)
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_CPU_QUOTA_PERCENT",
        bounds.cpu_quota_percent.to_string(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_MEMORY_MAX_BYTES",
        bounds.memory_bytes_max.to_string(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_MEMORY_SWAP_MAX_BYTES",
        MEMORY_SWAP_BYTES_MAX.to_string(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_TASKS_MAX",
        bounds.tasks_max.to_string(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_PROCESS_NICENESS",
        bounds.process_niceness.to_string(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_CARGO_JOBS_MAX",
        CARGO_JOBS_MAX.to_string(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_WALL_TIME_MINUTES",
        bounds.wall_time_minutes.to_string(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_TERMINATION_GRACE_SECONDS",
        bounds.termination_grace_seconds.to_string(),
    )
    .with_environment(
        "GTL_SERVER_HIGHLIGHTING_RSS_SAMPLE_INTERVAL_MICROSECONDS",
        RSS_SAMPLE_INTERVAL_MICROSECONDS.to_string(),
    )
    .with_current_directory(inputs.root);
    if let Some(massif) = inputs.massif {
        step = step
            .with_environment(
                "GTL_SERVER_HIGHLIGHTING_VALGRIND_BINARY",
                massif.valgrind.to_string_lossy(),
            )
            .with_environment(
                "GTL_SERVER_HIGHLIGHTING_MASSIF_OUTPUT",
                massif.output.to_string_lossy(),
            );
    }
    step
}

fn require_linux_systemd(operation: &str) -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("bounded {operation} requires Linux systemd user scopes");
    }
    Ok(())
}

fn required_executable(path: &str) -> Result<&Path> {
    let path = Path::new(path);
    ensure!(
        path.is_file(),
        "required executable is missing: {}",
        path.display()
    );
    Ok(path)
}

fn ensure_clean_repository(root: &Path) -> Result<()> {
    let status = git_output(root, &["status", "--porcelain=v1", "--untracked-files=all"])?;
    ensure!(
        status.is_empty(),
        "server highlighting measurement requires an exact committed source tree; dirty paths:\n{status}"
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

fn load_baseline(path: &Path, update: bool) -> Result<Option<ServerHighlightingReport>> {
    if !path.is_file() {
        ensure!(
            update,
            "server highlighting baseline is missing at {}; run `just bench-highlight --update` first",
            path.display()
        );
        return Ok(None);
    }
    match read_report(path) {
        Ok(report) => Ok(Some(report)),
        Err(error) if update => {
            eprintln!(
                "existing server highlighting baseline cannot be read and will be replaced: {error:#}"
            );
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn read_report(path: &Path) -> Result<ServerHighlightingReport> {
    let metadata = fs::metadata(path).with_context(|| {
        format!(
            "read server highlighting report metadata {}",
            path.display()
        )
    })?;
    ensure!(
        metadata.is_file(),
        "server highlighting report is not a file: {}",
        path.display()
    );
    ensure!(
        metadata.len() <= REPORT_BYTES_MAX,
        "server highlighting report {} exceeds {REPORT_BYTES_MAX} bytes",
        path.display()
    );
    let file = File::open(path)
        .with_context(|| format!("open server highlighting report {}", path.display()))?;
    serde_json::from_reader(std::io::BufReader::new(file))
        .with_context(|| format!("decode server highlighting report {}", path.display()))
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

fn write_bytes_atomically(path: &Path, bytes: &[u8], limit: u64) -> Result<()> {
    ensure!(
        bytes.len() as u64 <= limit,
        "evidence exceeds {limit} bytes"
    );
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("evidence path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create evidence directory {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary evidence in {}", parent.display()))?;
    temporary
        .write_all(bytes)
        .context("write temporary evidence")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync temporary evidence")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publish evidence {}", path.display()))?;
    Ok(())
}

fn print_comparison(comparison: &ServerHighlightingComparison) {
    println!("server highlighting baseline comparison (median [min, max]):");
    for workload in &comparison.workloads {
        print_workload(workload);
    }
}

fn print_workload(comparison: &WorkloadComparison) {
    for (temperature, sample) in [("cold", comparison.cold), ("warm", comparison.warm)] {
        println!("  {} {temperature}:", comparison.workload);
        print_metric("peak RSS MiB", sample.peak_rss_bytes, 1_048_576.0);
        print_metric("wall ms", sample.wall_time_microseconds, 1_000.0);
        print_metric(
            "server CPU ms",
            sample.server_cpu_time_microseconds,
            1_000.0,
        );
    }
}

fn print_metric(label: &str, metric: MetricDelta, divisor: f64) {
    let relative = metric
        .relative_change_percent
        .map_or_else(|| "n/a".to_owned(), |value| format!("{value:+.2}%"));
    println!(
        "    {label}: {:.2} [{:.2}, {:.2}] -> {:.2} [{:.2}, {:.2}] ({relative})",
        metric.baseline_median / divisor,
        metric.baseline_minimum / divisor,
        metric.baseline_maximum / divisor,
        metric.current_median / divisor,
        metric.current_minimum / divisor,
        metric.current_maximum / divisor,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_worker_records_resource_and_measurement_contracts() {
        let step = bounded_driver_step(DriverInputs {
            root: Path::new("/repo"),
            driver: Path::new("/repo/target/release/server-highlighting-driver"),
            server: Path::new("/repo/target/release/gtl-server"),
            report: Path::new("/repo/current.json"),
            launches: 3,
            source_commit: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            invocation: "just bench-highlight",
            bounds: BENCHMARK_BOUNDS,
            massif: None,
        });

        assert_eq!(step.program(), "/usr/bin/systemd-run");
        assert!(
            step.arguments()
                .contains(&"--property=CPUQuota=200%".to_owned())
        );
        assert!(
            step.arguments()
                .contains(&"--property=MemoryMax=4294967296".to_owned())
        );
        assert!(
            step.arguments()
                .contains(&"--property=MemorySwapMax=0".to_owned())
        );
        assert!(
            step.arguments()
                .contains(&"/repo/target/release/server-highlighting-driver".to_owned())
        );
        assert_eq!(
            environment_value(&step, "GTL_SERVER_HIGHLIGHTING_SERVER_BINARY"),
            Some("/repo/target/release/gtl-server")
        );
        assert_eq!(
            environment_value(&step, "GTL_SERVER_HIGHLIGHTING_LAUNCHES"),
            Some("3")
        );
        assert_eq!(
            environment_value(
                &step,
                "GTL_SERVER_HIGHLIGHTING_RSS_SAMPLE_INTERVAL_MICROSECONDS"
            ),
            Some("1000")
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

    fn environment_value<'a>(step: &'a Step, name: &str) -> Option<&'a str> {
        step.environment()
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}
