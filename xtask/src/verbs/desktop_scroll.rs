//! Bounded fixture and production-desktop measurement automation for realistic scrolling.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use gtl_benchmarks::desktop_scroll;

use super::{desktop_e2e, repository_root};
use crate::{process, task::Step};

const MEMORY_SWAP_BYTES_MAX: u64 = 0;
const CARGO_JOBS_MAX: usize = 1;
const RAYON_THREADS_MAX: usize = 2;
const GIT_OUTPUT_BYTES_MAX: usize = 64 * 1024;
const BENCHMARK_REPORT_DEFAULT: &str = ".artifacts/benchmarks/desktop-scroll.json";

const FIXTURE_BOUNDS: WorkerBounds = WorkerBounds {
    cpu_quota_percent: 200,
    memory_bytes_max: 2 * 1024 * 1024 * 1024,
    process_niceness: 10,
    tasks_max: 128,
    termination_grace_seconds: 10,
    wall_time_minutes: 10,
};

const BENCHMARK_BOUNDS: WorkerBounds = WorkerBounds {
    cpu_quota_percent: 200,
    memory_bytes_max: 4 * 1024 * 1024 * 1024,
    process_niceness: 10,
    tasks_max: 512,
    termination_grace_seconds: 15,
    wall_time_minutes: 60,
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
pub(crate) struct DesktopScrollBenchmarkArguments {
    /// Stable public identifier for this machine configuration; never use a hostname.
    #[arg(long, value_parser = parse_runner)]
    pub(crate) runner: String,
    /// Raw JSON evidence destination. The committed baseline report is reviewed separately.
    #[arg(long, default_value = BENCHMARK_REPORT_DEFAULT)]
    pub(crate) output: PathBuf,
    /// Independent release viewer launches to measure.
    #[arg(long, default_value_t = 3, value_parser = parse_launches)]
    pub(crate) launches: usize,
}

pub(crate) fn refresh_fixture() -> Result<()> {
    require_linux_systemd("desktop scroll fixture generation")?;
    let executable = std::env::current_exe().context("resolve xtask fixture worker executable")?;
    process::run_step(&bounded_fixture_worker_step(&executable))
}

pub(crate) fn run_fixture_worker() -> Result<()> {
    let root = repository_root().join(desktop_scroll::FIXTURE_RELATIVE_PATH);
    let evidence = desktop_scroll::refresh_fixture(&root)
        .with_context(|| format!("refresh desktop scroll fixture {}", root.display()))?;
    println!(
        "desktop scroll fixture: {} commits, {} files, {} input bytes",
        evidence.manifest.workload.commit_count,
        evidence.manifest.workload.distinct_file_count,
        evidence.fixture_input_bytes
    );
    Ok(())
}

pub(crate) fn run_benchmark(arguments: &DesktopScrollBenchmarkArguments) -> Result<()> {
    require_linux_systemd("desktop scroll benchmark")?;
    let root = repository_root();
    ensure_clean_repository(&root)?;
    let executable =
        std::env::current_exe().context("resolve xtask desktop benchmark worker executable")?;
    let source_commit = git_output(&root, &["rev-parse", "HEAD"])?;
    ensure!(
        source_commit.len() == 40 && source_commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Git returned an invalid source commit: {source_commit}"
    );
    let output = absolute_path(&root, &arguments.output);
    let invocation = format!(
        "just desktop-scroll-benchmark --runner {} --output {} --launches {}",
        arguments.runner,
        arguments.output.display(),
        arguments.launches
    );
    process::run_step(&bounded_benchmark_worker_step(
        &executable,
        &BenchmarkWorkerInputs {
            runner: &arguments.runner,
            output: &output,
            launches: arguments.launches,
            source_commit: &source_commit,
            invocation: &invocation,
        },
    ))
}

pub(crate) fn run_benchmark_worker() -> Result<()> {
    desktop_e2e::run_scroll_benchmark()
}

struct BenchmarkWorkerInputs<'a> {
    runner: &'a str,
    output: &'a Path,
    launches: usize,
    source_commit: &'a str,
    invocation: &'a str,
}

fn bounded_fixture_worker_step(executable: &Path) -> Step {
    bounded_worker_step(
        "bounded desktop scroll fixture generation",
        executable,
        "desktop-scroll-fixture-worker",
        FIXTURE_BOUNDS,
    )
}

fn bounded_benchmark_worker_step(executable: &Path, inputs: &BenchmarkWorkerInputs<'_>) -> Step {
    let bounds = BENCHMARK_BOUNDS;
    bounded_worker_step(
        "bounded production desktop scroll benchmark",
        executable,
        "desktop-scroll-benchmark-worker",
        bounds,
    )
    .with_environment("CARGO_TERM_QUIET", "true")
    .with_environment("GTL_DESKTOP_SCROLL_RUNNER", inputs.runner)
    .with_environment(
        "GTL_DESKTOP_SCROLL_REPORT_PATH",
        inputs.output.to_string_lossy(),
    )
    .with_environment("GTL_DESKTOP_SCROLL_LAUNCHES", inputs.launches.to_string())
    .with_environment("GTL_DESKTOP_SCROLL_SOURCE_COMMIT", inputs.source_commit)
    .with_environment("GTL_DESKTOP_SCROLL_INVOCATION", inputs.invocation)
    .with_environment(
        "GTL_DESKTOP_SCROLL_CPU_QUOTA_PERCENT",
        bounds.cpu_quota_percent.to_string(),
    )
    .with_environment(
        "GTL_DESKTOP_SCROLL_MEMORY_MAX_BYTES",
        bounds.memory_bytes_max.to_string(),
    )
    .with_environment(
        "GTL_DESKTOP_SCROLL_MEMORY_SWAP_MAX_BYTES",
        MEMORY_SWAP_BYTES_MAX.to_string(),
    )
    .with_environment("GTL_DESKTOP_SCROLL_TASKS_MAX", bounds.tasks_max.to_string())
    .with_environment(
        "GTL_DESKTOP_SCROLL_PROCESS_NICENESS",
        bounds.process_niceness.to_string(),
    )
    .with_environment(
        "GTL_DESKTOP_SCROLL_CARGO_JOBS_MAX",
        CARGO_JOBS_MAX.to_string(),
    )
    .with_environment(
        "GTL_DESKTOP_SCROLL_RAYON_THREADS_MAX",
        RAYON_THREADS_MAX.to_string(),
    )
    .with_environment(
        "GTL_DESKTOP_SCROLL_WALL_TIME_MINUTES",
        bounds.wall_time_minutes.to_string(),
    )
    .with_environment(
        "GTL_DESKTOP_SCROLL_TERMINATION_GRACE_SECONDS",
        bounds.termination_grace_seconds.to_string(),
    )
}

fn bounded_worker_step(
    label: &str,
    executable: &Path,
    worker_verb: &str,
    bounds: WorkerBounds,
) -> Step {
    Step::new(
        label,
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
            executable.to_string_lossy().into_owned(),
            worker_verb.to_owned(),
        ],
    )
    .with_environment("CARGO_BUILD_JOBS", CARGO_JOBS_MAX.to_string())
    .with_environment("RAYON_NUM_THREADS", RAYON_THREADS_MAX.to_string())
    .with_current_directory(repository_root())
}

fn require_linux_systemd(operation: &str) -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("bounded {operation} requires Linux systemd user scopes");
    }
    Ok(())
}

fn ensure_clean_repository(root: &Path) -> Result<()> {
    let status = git_output(root, &["status", "--porcelain=v1", "--untracked-files=all"])?;
    ensure!(
        status.is_empty(),
        "desktop scroll benchmark requires an exact committed source tree; dirty paths:\n{status}"
    );
    Ok(())
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
    if !output.status.success() {
        bail!(
            "Git {} failed (exit {}): {}{}",
            arguments.join(" "),
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout)
        .context("Git output was not UTF-8")
        .map(|value| value.trim().to_owned())
}

fn absolute_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn parse_runner(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(
            "runner must contain 1 through 64 lowercase ASCII letters, digits, or hyphens"
                .to_owned(),
        );
    }
    Ok(value.to_owned())
}

fn parse_launches(value: &str) -> Result<usize, String> {
    let launches = value
        .parse::<usize>()
        .map_err(|error| format!("launch count is not an integer: {error}"))?;
    if !(3..=10).contains(&launches) {
        return Err("launch count must be between 3 and 10".to_owned());
    }
    Ok(launches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn fixture_worker_enforces_repository_resource_bounds() {
        let step = bounded_fixture_worker_step(Path::new("/repo/target/debug/xtask"));

        assert_eq!(step.program(), "/usr/bin/systemd-run");
        assert_eq!(
            step.arguments(),
            [
                "--user",
                "--scope",
                "--quiet",
                "--collect",
                "--property=CPUQuota=200%",
                "--property=MemoryMax=2147483648",
                "--property=MemorySwapMax=0",
                "--property=TasksMax=128",
                "/usr/bin/nice",
                "-n",
                "10",
                "/usr/bin/timeout",
                "--signal=TERM",
                "--kill-after=10s",
                "10m",
                "/repo/target/debug/xtask",
                "desktop-scroll-fixture-worker",
            ]
        );
        assert_eq!(
            step.environment(),
            [
                ("CARGO_BUILD_JOBS".to_owned(), "1".to_owned()),
                ("RAYON_NUM_THREADS".to_owned(), "2".to_owned()),
            ]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn benchmark_worker_enforces_measurement_bounds_and_identity() {
        let step = bounded_benchmark_worker_step(
            Path::new("/repo/target/debug/xtask"),
            &BenchmarkWorkerInputs {
                runner: "example-runner",
                output: Path::new("/repo/.artifacts/scroll.json"),
                launches: 3,
                source_commit: "0123456789012345678901234567890123456789",
                invocation: "just desktop-scroll-benchmark --runner example-runner",
            },
        );

        assert_eq!(
            step.arguments(),
            [
                "--user",
                "--scope",
                "--quiet",
                "--collect",
                "--property=CPUQuota=200%",
                "--property=MemoryMax=4294967296",
                "--property=MemorySwapMax=0",
                "--property=TasksMax=512",
                "/usr/bin/nice",
                "-n",
                "10",
                "/usr/bin/timeout",
                "--signal=TERM",
                "--kill-after=15s",
                "60m",
                "/repo/target/debug/xtask",
                "desktop-scroll-benchmark-worker",
            ]
        );
        assert!(
            step.environment()
                .contains(&("GTL_DESKTOP_SCROLL_LAUNCHES".to_owned(), "3".to_owned()))
        );
        assert!(step.environment().contains(&(
            "GTL_DESKTOP_SCROLL_MEMORY_MAX_BYTES".to_owned(),
            "4294967296".to_owned()
        )));
        assert!(step.environment().contains(&(
            "GTL_DESKTOP_SCROLL_REPORT_PATH".to_owned(),
            "/repo/.artifacts/scroll.json".to_owned()
        )));
    }

    #[test]
    fn benchmark_argument_bounds_reject_hostnames_and_short_runs() {
        assert!(parse_runner("benchmark-host").is_ok());
        assert!(parse_runner("devbox.example.invalid").is_err());
        assert!(parse_runner("Runner").is_err());
        assert!(parse_launches("3").is_ok());
        assert!(parse_launches("2").is_err());
        assert!(parse_launches("11").is_err());
    }
}
