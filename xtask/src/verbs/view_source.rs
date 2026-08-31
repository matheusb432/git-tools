//! Release-process benchmark for demand-loaded full-context diff source.

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    path::Path,
    process::Command,
};

use anyhow::{Context, Result, ensure};
use clap::Args;
use gtl_benchmarks::view_source::{
    LaunchFragment, ViewSourceComparison, ViewSourceDescriptor, ViewSourceReport, compare_reports,
    ensure_compatible, summarize_current, validate_report,
};
use serde::{Serialize, de::DeserializeOwned};

use super::{cargo_target_directory, repository_root};
use crate::{process, task::Step};

const BENCHMARK_LAUNCHES: usize = 3;
const CARGO_JOBS_MAX: usize = 1;
const REPORT_BYTES_MAX: u64 = 16 * 1_024 * 1_024;
const GIT_OUTPUT_BYTES_MAX: usize = 64 * 1_024;
const BENCHMARK_BASELINE_RELATIVE_PATH: &str = ".artifacts/benchmarks/view-source/baseline.json";
const BENCHMARK_CURRENT_RELATIVE_PATH: &str = ".artifacts/benchmarks/view-source/current.json";
const CPU_QUOTA_PERCENT: usize = 200;
const MEMORY_MAX_BYTES: u64 = 4 * 1_024 * 1_024 * 1_024;
const TASKS_MAX: usize = 256;
const PROCESS_NICENESS: usize = 10;
const WALL_TIME_MINUTES: u64 = 10;
const TERMINATION_GRACE_SECONDS: u64 = 15;

#[derive(Args, Debug)]
pub(crate) struct ViewSourceBenchmarkArguments {
    /// Compare the current result, then replace the local baseline after a successful run.
    #[arg(long)]
    pub(crate) update: bool,
}

pub(crate) fn run_benchmark(arguments: &ViewSourceBenchmarkArguments) -> Result<()> {
    ensure!(
        cfg!(target_os = "linux"),
        "bounded view-source measurement requires Linux systemd user scopes"
    );
    let root = repository_root();
    let baseline_path = root.join(BENCHMARK_BASELINE_RELATIVE_PATH);
    let current_path = root.join(BENCHMARK_CURRENT_RELATIVE_PATH);
    let baseline = load_baseline(&baseline_path, arguments.update)?;
    ensure_clean_repository(&root)?;
    build_release_driver(&root)?;
    let target = cargo_target_directory(&root)?;
    let driver = target.join("release/view-source-driver");
    let temporary = tempfile::Builder::new()
        .prefix("view-source-xtask-")
        .tempdir()
        .context("create view-source orchestration directory")?;
    let descriptor_path = temporary.path().join("descriptor.json");
    process::run_step(&bounded_driver_step(
        "inspect view-source fixtures",
        &driver,
        [
            "describe".to_owned(),
            "--output".to_owned(),
            descriptor_path.to_string_lossy().into_owned(),
        ],
        &root,
    ))?;
    let descriptor: ViewSourceDescriptor = read_json(&descriptor_path, "view-source descriptor")?;
    let baseline = preflight_baseline(baseline, &descriptor, arguments.update)?;

    let source_commit = source_commit(&root)?;
    let invocation = if arguments.update {
        "just bench-view-source --update"
    } else {
        "just bench-view-source"
    };
    let mut fragments = Vec::with_capacity(BENCHMARK_LAUNCHES);
    for launch in 1..=BENCHMARK_LAUNCHES {
        let output = temporary.path().join(format!("launch-{launch}.json"));
        process::run_step(&bounded_driver_step(
            format!("measure view-source launch {launch}"),
            &driver,
            [
                "measure".to_owned(),
                "--launch".to_owned(),
                launch.to_string(),
                "--source-commit".to_owned(),
                source_commit.clone(),
                "--invocation".to_owned(),
                invocation.to_owned(),
                "--output".to_owned(),
                output.to_string_lossy().into_owned(),
            ],
            &root,
        ))?;
        fragments.push(read_json(&output, "view-source launch fragment")?);
    }
    let current = assemble_report(descriptor, fragments)?;
    write_json_atomically(&current_path, &current)?;
    print_current(&current);
    if let Some(baseline) = baseline {
        let comparison = compare_reports(&baseline, &current)?;
        print_comparison(&comparison);
    }
    if arguments.update {
        replace_file_atomically(&current_path, &baseline_path)?;
        println!("view-source baseline: {}", baseline_path.display());
    }
    println!("view-source current report: {}", current_path.display());
    Ok(())
}

fn build_release_driver(root: &Path) -> Result<()> {
    process::run_step(
        &Step::new(
            "release view-source benchmark driver",
            "cargo",
            [
                "build",
                "--locked",
                "--release",
                "-p",
                "gtl-benchmarks",
                "--bin",
                "view-source-driver",
            ],
        )
        .with_environment("CARGO_BUILD_JOBS", CARGO_JOBS_MAX.to_string())
        .with_current_directory(root),
    )
}

fn bounded_driver_step(
    label: impl Into<String>,
    driver: &Path,
    arguments: impl IntoIterator<Item = String>,
    root: &Path,
) -> Step {
    Step::new(
        label,
        "/usr/bin/systemd-run",
        [
            "--user".to_owned(),
            "--scope".to_owned(),
            "--quiet".to_owned(),
            "--collect".to_owned(),
            format!("--property=CPUQuota={CPU_QUOTA_PERCENT}%"),
            format!("--property=MemoryMax={MEMORY_MAX_BYTES}"),
            "--property=MemorySwapMax=0".to_owned(),
            format!("--property=TasksMax={TASKS_MAX}"),
            "/usr/bin/nice".to_owned(),
            "-n".to_owned(),
            PROCESS_NICENESS.to_string(),
            "/usr/bin/timeout".to_owned(),
            "--signal=TERM".to_owned(),
            format!("--kill-after={TERMINATION_GRACE_SECONDS}s"),
            format!("{WALL_TIME_MINUTES}m"),
            driver.to_string_lossy().into_owned(),
        ],
    )
    .with_arguments(arguments)
    .with_current_directory(root)
}

fn load_baseline(path: &Path, update: bool) -> Result<Option<ViewSourceReport>> {
    if !path.is_file() {
        ensure!(
            update,
            "view-source baseline is missing at {}; run `just bench-view-source --update` first",
            path.display()
        );
        return Ok(None);
    }
    match read_json(path, "view-source baseline").and_then(|report| {
        validate_report(&report).context("validate view-source baseline")?;
        Ok(report)
    }) {
        Ok(report) => Ok(Some(report)),
        Err(error) if update => {
            eprintln!(
                "existing view-source baseline cannot be used and will be replaced: {error:#}"
            );
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn preflight_baseline(
    baseline: Option<ViewSourceReport>,
    descriptor: &ViewSourceDescriptor,
    update: bool,
) -> Result<Option<ViewSourceReport>> {
    let Some(baseline) = baseline else {
        return Ok(None);
    };
    match ensure_compatible(&baseline, descriptor) {
        Ok(()) => Ok(Some(baseline)),
        Err(error) if update => {
            eprintln!(
                "existing view-source baseline is not comparable and will be replaced: {error}"
            );
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

fn assemble_report(
    descriptor: ViewSourceDescriptor,
    fragments: Vec<LaunchFragment>,
) -> Result<ViewSourceReport> {
    ensure!(
        fragments.len() == BENCHMARK_LAUNCHES,
        "expected {BENCHMARK_LAUNCHES} view-source fragments"
    );
    let source = fragments
        .first()
        .context("view-source fragments are empty")?
        .source
        .clone();
    let mut launches = Vec::with_capacity(fragments.len());
    for (index, fragment) in fragments.into_iter().enumerate() {
        ensure!(
            fragment.source == source,
            "view-source launch {} changed source identity",
            index + 1
        );
        ensure!(
            fragment.descriptor == descriptor,
            "view-source launch {} changed fixture, protocol, or runner identity",
            index + 1
        );
        ensure!(
            fragment.launch.launch == index + 1,
            "view-source launch sequence changed at {}",
            index + 1
        );
        launches.push(fragment.launch);
    }
    ensure!(
        source.profile == descriptor.profile,
        "view-source source and descriptor profiles differ"
    );
    let report = ViewSourceReport {
        format_version: descriptor.format_version,
        benchmark: descriptor.benchmark,
        source,
        fixtures: descriptor.fixtures,
        protocol: descriptor.protocol,
        runner: descriptor.runner,
        launches,
    };
    validate_report(&report).context("validate completed view-source report")?;
    Ok(report)
}

fn print_current(report: &ViewSourceReport) {
    println!("view-source current medians across {BENCHMARK_LAUNCHES} launches:");
    for summary in summarize_current(report) {
        println!(
            "  {}: compact wall {:.0} us, CPU {:.0} us, cache {} B",
            summary.workload,
            summary.compact.wall_time_microseconds.median,
            summary.compact.process_tree_cpu_time_microseconds.median,
            summary.compact_cache_weight_bytes,
        );
        println!(
            "    eager wall {:.0} us, CPU {:.0} us, cache {} B; compact change wall {}, CPU {}, cache {}",
            summary.eager_full_context.wall_time_microseconds.median,
            summary
                .eager_full_context
                .process_tree_cpu_time_microseconds
                .median,
            summary.full_context_cache_weight_bytes,
            format_percent(summary.compact_wall_change_from_eager_percent),
            format_percent(summary.compact_cpu_change_from_eager_percent),
            format_percent(summary.compact_cache_change_from_eager_percent),
        );
        println!(
            "    first Compact-to-Full transition wall {:.0} us, CPU {:.0} us",
            summary
                .first_full_context_transition
                .wall_time_microseconds
                .median,
            summary
                .first_full_context_transition
                .process_tree_cpu_time_microseconds
                .median,
        );
    }
}

fn print_comparison(comparison: &ViewSourceComparison) {
    println!("view-source historical median changes:");
    for workload in &comparison.workloads {
        for operation in &workload.operations {
            println!(
                "  {} {}: wall {}, CPU {}",
                workload.workload,
                operation.operation,
                format_percent(operation.wall_time_microseconds.relative_change_percent),
                format_percent(
                    operation
                        .process_tree_cpu_time_microseconds
                        .relative_change_percent
                ),
            );
        }
    }
}

fn format_percent(change: Option<f64>) -> String {
    change.map_or_else(|| "n/a".to_owned(), |change| format!("{change:+.1}%"))
}

fn ensure_clean_repository(root: &Path) -> Result<()> {
    let status = git_output(root, &["status", "--porcelain=v1", "--untracked-files=all"])?;
    ensure!(
        status.is_empty(),
        "view-source measurement requires an exact committed source tree; dirty paths:\n{status}"
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

fn read_json<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T> {
    let metadata = fs::metadata(path).with_context(|| format!("read {label} metadata"))?;
    ensure!(
        metadata.is_file(),
        "{label} is not a file: {}",
        path.display()
    );
    ensure!(
        metadata.len() <= REPORT_BYTES_MAX,
        "{label} exceeds {REPORT_BYTES_MAX} bytes: {}",
        path.display()
    );
    let file = File::open(path).with_context(|| format!("open {label} {}", path.display()))?;
    serde_json::from_reader(std::io::BufReader::new(file))
        .with_context(|| format!("decode {label} {}", path.display()))
}

fn write_json_atomically(path: &Path, report: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("view-source report has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create view-source report directory {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "create temporary view-source report in {}",
            parent.display()
        )
    })?;
    serde_json::to_writer_pretty(&mut temporary, report).context("encode view-source report")?;
    temporary
        .write_all(b"\n")
        .context("terminate view-source report")?;
    temporary.flush().context("flush view-source report")?;
    ensure!(
        temporary.as_file().metadata()?.len() <= REPORT_BYTES_MAX,
        "view-source report exceeds {REPORT_BYTES_MAX} bytes"
    );
    temporary
        .as_file()
        .sync_all()
        .context("sync view-source report")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publish view-source report {}", path.display()))?;
    Ok(())
}

fn replace_file_atomically(source: &Path, destination: &Path) -> Result<()> {
    let source_file =
        File::open(source).with_context(|| format!("open source report {}", source.display()))?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("view-source baseline has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create view-source baseline directory {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "create temporary view-source baseline in {}",
            parent.display()
        )
    })?;
    let copied = std::io::copy(&mut source_file.take(REPORT_BYTES_MAX + 1), &mut temporary)
        .context("copy view-source baseline")?;
    ensure!(
        copied <= REPORT_BYTES_MAX,
        "view-source report exceeds size bound"
    );
    temporary
        .as_file()
        .sync_all()
        .context("sync view-source baseline")?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)
        .with_context(|| format!("publish view-source baseline {}", destination.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_launch_is_bounded_and_uses_the_release_driver() {
        let step = bounded_driver_step(
            "measure",
            Path::new("/repo/target/release/view-source-driver"),
            ["measure".to_owned(), "--launch".to_owned(), "1".to_owned()],
            Path::new("/repo"),
        );

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
        assert!(step.arguments().contains(&"10m".to_owned()));
        assert!(
            step.arguments()
                .contains(&"/repo/target/release/view-source-driver".to_owned())
        );
        assert_eq!(step.current_directory(), Some(Path::new("/repo")));
    }

    #[test]
    fn baseline_replacement_is_atomic_and_exact() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("current.json");
        let destination = directory.path().join("baseline.json");
        fs::write(&source, b"current report\n").unwrap();

        replace_file_atomically(&source, &destination).unwrap();

        assert_eq!(fs::read(destination).unwrap(), b"current report\n");
    }
}
