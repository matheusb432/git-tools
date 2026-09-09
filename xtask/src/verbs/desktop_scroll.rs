//! Bounded fixture and production-desktop measurement automation for realistic scrolling.

use std::{
    fs::{self, File},
    io::Read as _,
    path::Path,
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use gtl_benchmarks::desktop_scroll::{
    self, DesktopScrollComparison, DesktopScrollReport, MetricDelta, PanelComparison,
    compare_reports,
};

use super::{desktop_e2e, repository_root};
use crate::{process, task::Step};

const MEMORY_SWAP_BYTES_MAX: u64 = 0;
const CARGO_JOBS_MAX: usize = 1;
const RAYON_THREADS_MAX: usize = 2;
const GIT_OUTPUT_BYTES_MAX: usize = 64 * 1024;
const BENCHMARK_LAUNCHES: usize = 3;
const BENCHMARK_BASELINE_RELATIVE_PATH: &str = ".artifacts/benchmarks/desktop-scroll/baseline.json";
const BENCHMARK_CURRENT_RELATIVE_PATH: &str = ".artifacts/benchmarks/desktop-scroll/current.json";
const BENCHMARK_REPORT_BYTES_MAX: u64 = 16 * 1024 * 1024;

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
    wall_time_minutes: 40,
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
    /// Compare the current result, then replace the local baseline after a successful run.
    #[arg(long)]
    pub(crate) update: bool,
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
    let baseline_path = root.join(BENCHMARK_BASELINE_RELATIVE_PATH);
    let current_path = root.join(BENCHMARK_CURRENT_RELATIVE_PATH);
    let baseline = load_baseline(&baseline_path, arguments.update)?;
    ensure_clean_repository(&root)?;
    let executable =
        std::env::current_exe().context("resolve xtask desktop benchmark worker executable")?;
    let source_commit = git_output(&root, &["rev-parse", "HEAD"])?;
    ensure!(
        source_commit.len() == 40 && source_commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Git returned an invalid source commit: {source_commit}"
    );
    let invocation = if arguments.update {
        "just bench-scroll --update"
    } else {
        "just bench-scroll"
    };
    process::run_step(&bounded_benchmark_worker_step(
        &executable,
        &BenchmarkWorkerInputs {
            output: &current_path,
            launches: BENCHMARK_LAUNCHES,
            source_commit: &source_commit,
            invocation,
        },
    ))?;

    let current = read_report(&current_path)?;
    if let Some(baseline) = baseline {
        match compare_reports(&baseline, &current) {
            Ok(comparison) => print_comparison(&comparison),
            Err(error) if arguments.update => {
                eprintln!("existing desktop scroll baseline is not comparable: {error}");
            }
            Err(error) => return Err(error.into()),
        }
    }
    if arguments.update {
        replace_report_atomically(&current_path, &baseline_path)?;
        println!("desktop scroll baseline: {}", baseline_path.display());
    }
    Ok(())
}

pub(crate) fn run_benchmark_worker() -> Result<()> {
    desktop_e2e::run_scroll_benchmark()
}

struct BenchmarkWorkerInputs<'a> {
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

fn load_baseline(path: &Path, update: bool) -> Result<Option<DesktopScrollReport>> {
    if !path.is_file() {
        ensure!(
            update,
            "desktop scroll baseline is missing at {}; run `just bench-scroll --update` first",
            path.display()
        );
        return Ok(None);
    }
    match read_report(path) {
        Ok(report) => Ok(Some(report)),
        Err(error) if update => {
            eprintln!(
                "existing desktop scroll baseline cannot be read and will be replaced: {error:#}"
            );
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn read_report(path: &Path) -> Result<DesktopScrollReport> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("read desktop scroll report metadata {}", path.display()))?;
    ensure!(
        metadata.is_file(),
        "desktop scroll report is not a file: {}",
        path.display()
    );
    ensure!(
        metadata.len() <= BENCHMARK_REPORT_BYTES_MAX,
        "desktop scroll report {} exceeds {BENCHMARK_REPORT_BYTES_MAX} bytes",
        path.display()
    );
    let file = File::open(path)
        .with_context(|| format!("open desktop scroll report {}", path.display()))?;
    serde_json::from_reader(std::io::BufReader::new(file))
        .with_context(|| format!("decode desktop scroll report {}", path.display()))
}

fn replace_report_atomically(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("desktop scroll baseline path has no parent")?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "create desktop scroll baseline directory {}",
            parent.display()
        )
    })?;
    let mut source_file = File::open(source)
        .with_context(|| format!("open current desktop scroll report {}", source.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "create temporary desktop scroll baseline in {}",
            parent.display()
        )
    })?;
    let copied_bytes = std::io::copy(
        &mut source_file.by_ref().take(BENCHMARK_REPORT_BYTES_MAX + 1),
        &mut temporary,
    )
    .context("copy current desktop scroll report into temporary baseline")?;
    ensure!(
        copied_bytes <= BENCHMARK_REPORT_BYTES_MAX,
        "current desktop scroll report exceeds {BENCHMARK_REPORT_BYTES_MAX} bytes"
    );
    temporary
        .as_file()
        .sync_all()
        .context("sync temporary desktop scroll baseline")?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)
        .with_context(|| format!("publish desktop scroll baseline {}", destination.display()))?;
    Ok(())
}

fn print_comparison(comparison: &DesktopScrollComparison) {
    println!("desktop scroll comparison (current vs baseline)");
    println!(
        "{:<34} {:>14} {:>14} {:>10}",
        "metric", "baseline", "current", "change"
    );
    print_metric(
        "many-file readiness wall time",
        comparison.readiness_wall_time_milliseconds,
        |value| format!("{value:.2} ms"),
    );
    print_metric(
        "many-file readiness process CPU time",
        comparison.readiness_process_cpu_time_milliseconds,
        |value| format!("{value:.2} ms"),
    );
    print_metric(
        "many-file loading viewer RSS",
        comparison.viewer_loading_peak_rss_bytes,
        |value| format!("{:.1} MiB", value / 1024.0 / 1024.0),
    );
    print_panel("many-file loading", comparison.loading);
    print_panel("many-file diff document", comparison.diff_document);
    print_panel("changed files", comparison.changed_files);
    print_panel("commits", comparison.commits);
    print_metric("many-file peak RSS", comparison.peak_rss_bytes, |value| {
        format!("{:.1} MiB", value / 1024.0 / 1024.0)
    });
    print_metric(
        "single-file readiness wall time",
        comparison.single_file_readiness_wall_time_milliseconds,
        |value| format!("{value:.2} ms"),
    );
    print_metric(
        "single-file readiness process CPU time",
        comparison.single_file_readiness_process_cpu_time_milliseconds,
        |value| format!("{value:.2} ms"),
    );
    print_metric(
        "single-file loading viewer RSS",
        comparison.single_file_viewer_loading_peak_rss_bytes,
        |value| format!("{:.1} MiB", value / 1024.0 / 1024.0),
    );
    print_panel("single-file loading", comparison.single_file_loading);
    print_panel(
        "single-file diff document",
        comparison.single_file_diff_document,
    );
    print_metric(
        "single-file peak RSS",
        comparison.single_file_peak_rss_bytes,
        |value| format!("{:.1} MiB", value / 1024.0 / 1024.0),
    );
}

fn print_panel(name: &str, comparison: PanelComparison) {
    for (metric, delta) in [
        ("p50 frame gap", comparison.frame_gap_ms.p50),
        ("p95 frame gap", comparison.frame_gap_ms.p95),
        ("p99 frame gap", comparison.frame_gap_ms.p99),
    ] {
        print_metric(&format!("{name} {metric}"), delta, |value| {
            format!("{value:.2} ms")
        });
    }
    print_metric(
        &format!("{name} frames over 33 ms"),
        comparison.frames_exceeding_33_ms,
        |value| format!("{value:.0}"),
    );
}

fn print_metric(label: &str, delta: MetricDelta, format_value: impl Fn(f64) -> String) {
    let change = delta
        .relative_change_percent
        .map_or_else(|| "n/a".to_owned(), |percent| format!("{percent:+.2}%"));
    println!(
        "{label:<34} {:>14} {:>14} {change:>10}",
        format_value(delta.baseline),
        format_value(delta.current)
    );
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
                output: Path::new("/repo/.artifacts/scroll.json"),
                launches: 3,
                source_commit: "0123456789012345678901234567890123456789",
                invocation: "just bench-scroll",
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
                "40m",
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
    fn baseline_replacement_is_atomic_and_exact() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("current.json");
        let destination = directory.path().join("baseline.json");
        fs::write(&source, b"current report\n").unwrap();
        fs::write(&destination, b"old report\n").unwrap();

        replace_report_atomically(&source, &destination).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"current report\n");
    }

    #[test]
    fn comparison_requires_an_existing_baseline_before_measurement() {
        let directory = tempfile::tempdir().unwrap();
        let baseline = directory.path().join("baseline.json");

        let error = load_baseline(&baseline, false).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("run `just bench-scroll --update` first")
        );
    }
}
