//! Shared Rust benchmark entrypoint.

use anyhow::{Result, bail};
use clap::Args;
use gtl_benchmarks::{Benchmark, BenchmarkCase, PACKAGE_NAME};

use crate::{process, task::Step};

const FAST_SAMPLE_SIZE: usize = 100;
const FAST_WARM_UP_SECONDS: u64 = 5;
const FAST_MEASUREMENT_SECONDS: u64 = 5;
const MINIMUM_SAMPLE_SIZE: usize = 10;
const BOUNDED_CARGO_JOBS: usize = 1;
const BOUNDED_CPU_QUOTA_PERCENT: usize = 200;
const BOUNDED_MEMORY_BYTES_MAX: u64 = 4 * 1024 * 1024 * 1024;
const BOUNDED_NICENESS: usize = 10;
const BOUNDED_RAYON_THREADS: usize = 2;
const BOUNDED_TASKS_MAX: usize = 512;
const BOUNDED_TERMINATION_GRACE_SECONDS: u64 = 10;
const BOUNDED_WALL_TIME_MINUTES: u64 = 15;

#[derive(Args, Debug)]
pub(crate) struct BenchArguments {
    /// Benchmark target to run.
    #[arg(long, value_enum, default_value_t = Benchmark::ViewerRender)]
    benchmark: Benchmark,
    /// Run the selected target's stable, concise Criterion preset.
    #[arg(
        long,
        conflicts_with_all = ["case", "sample_size", "criterion_arguments"]
    )]
    fast: bool,
    /// Run exactly one case owned by the selected benchmark.
    #[arg(long, value_enum, conflicts_with = "fast")]
    case: Option<BenchmarkCase>,
    /// Override the benchmark target's Criterion sample count.
    #[arg(long, value_parser = parse_sample_size, conflicts_with = "fast")]
    sample_size: Option<usize>,
    /// Run in a Linux user scope capped at two CPUs, 4 GiB of memory, and 15 minutes.
    #[arg(long)]
    bounded: bool,
    /// Additional arguments forwarded to Criterion after `--`.
    #[arg(last = true, allow_hyphen_values = true, conflicts_with = "fast")]
    criterion_arguments: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BenchmarkSelection {
    Standard {
        case: Option<BenchmarkCase>,
        sample_size: Option<usize>,
    },
    Fast,
}

impl BenchArguments {
    fn selection(&self) -> BenchmarkSelection {
        if self.fast {
            BenchmarkSelection::Fast
        } else {
            BenchmarkSelection::Standard {
                case: self.case,
                sample_size: self.sample_size,
            }
        }
    }
}

fn parse_sample_size(value: &str) -> Result<usize, String> {
    let sample_size = value
        .parse::<usize>()
        .map_err(|_| "sample size must be a positive integer".to_string())?;
    if sample_size < MINIMUM_SAMPLE_SIZE {
        return Err(format!(
            "sample size must be at least {MINIMUM_SAMPLE_SIZE}"
        ));
    }
    Ok(sample_size)
}

pub(crate) fn run(arguments: &BenchArguments) -> Result<()> {
    let step = benchmark_step(
        arguments.benchmark,
        arguments.selection(),
        arguments.bounded,
        &arguments.criterion_arguments,
    )?;
    process::run_step(&step)
}

fn benchmark_step(
    benchmark: Benchmark,
    selection: BenchmarkSelection,
    bounded: bool,
    criterion_arguments: &[String],
) -> Result<Step> {
    let mut arguments = vec!["bench".to_owned()];
    if matches!(selection, BenchmarkSelection::Fast) {
        arguments.push("--quiet".to_owned());
    }
    arguments.extend(
        [
            "-p",
            PACKAGE_NAME,
            "--bench",
            benchmark.cargo_target(),
            "--",
        ]
        .into_iter()
        .map(str::to_owned),
    );

    match selection {
        BenchmarkSelection::Standard { case, sample_size } => {
            if let Some(case) = case {
                if case.benchmark() != benchmark {
                    bail!("benchmark case {case} belongs to {}", case.benchmark());
                }
                arguments.extend([case.to_string(), "--exact".to_owned()]);
            }
            if let Some(sample_size) = sample_size {
                arguments.extend(["--sample-size".to_owned(), sample_size.to_string()]);
            }
            arguments.extend(criterion_arguments.iter().cloned());
        }
        BenchmarkSelection::Fast => {
            let fast_cases = benchmark.fast_cases();
            if fast_cases.is_empty() {
                bail!("benchmark {benchmark} has no fast preset");
            }
            arguments.extend([
                exact_case_filter(fast_cases),
                "--sample-size".to_owned(),
                FAST_SAMPLE_SIZE.to_string(),
                "--warm-up-time".to_owned(),
                FAST_WARM_UP_SECONDS.to_string(),
                "--measurement-time".to_owned(),
                FAST_MEASUREMENT_SECONDS.to_string(),
                "--discard-baseline".to_owned(),
                "--noplot".to_owned(),
                "--output-format".to_owned(),
                "bencher".to_owned(),
                "--color".to_owned(),
                "never".to_owned(),
            ]);
        }
    }

    let mut step = if bounded {
        if !cfg!(target_os = "linux") {
            bail!("bounded benchmarks require Linux systemd user scopes");
        }
        bounded_benchmark_step(arguments)
    } else {
        Step::new("benchmark", "cargo", arguments)
    };
    if benchmark == Benchmark::GrpcRequests {
        step = step
            .with_environment("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .with_environment("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .with_environment("GIT_CONFIG_GLOBAL", "/dev/null")
            .with_environment("GIT_CONFIG_NOSYSTEM", "1")
            .with_environment("GIT_TERMINAL_PROMPT", "0")
            .with_environment("LC_ALL", "C")
            .with_environment("TZ", "UTC");
    }
    Ok(step.without_environment(["DISPLAY", "WAYLAND_DISPLAY"]))
}

fn bounded_benchmark_step(cargo_arguments: Vec<String>) -> Step {
    let mut arguments = vec![
        "--user".to_owned(),
        "--scope".to_owned(),
        "--quiet".to_owned(),
        "--collect".to_owned(),
        format!("--property=CPUQuota={BOUNDED_CPU_QUOTA_PERCENT}%"),
        format!("--property=MemoryMax={BOUNDED_MEMORY_BYTES_MAX}"),
        "--property=MemorySwapMax=0".to_owned(),
        format!("--property=TasksMax={BOUNDED_TASKS_MAX}"),
        "/usr/bin/nice".to_owned(),
        "-n".to_owned(),
        BOUNDED_NICENESS.to_string(),
        "/usr/bin/timeout".to_owned(),
        "--signal=TERM".to_owned(),
        format!("--kill-after={BOUNDED_TERMINATION_GRACE_SECONDS}s"),
        format!("{BOUNDED_WALL_TIME_MINUTES}m"),
        "cargo".to_owned(),
    ];
    arguments.extend(cargo_arguments);
    Step::new("bounded benchmark", "/usr/bin/systemd-run", arguments)
        .with_environment("CARGO_BUILD_JOBS", BOUNDED_CARGO_JOBS.to_string())
        .with_environment("RAYON_NUM_THREADS", BOUNDED_RAYON_THREADS.to_string())
}

fn exact_case_filter(cases: &[BenchmarkCase]) -> String {
    let case_names = cases
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("|");
    format!("^({case_names})$")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argument_strings(step: &Step) -> Vec<&str> {
        step.arguments().iter().map(String::as_str).collect()
    }

    #[test]
    fn benchmark_step_forwards_selected_case_sample_size_and_criterion_arguments() {
        let step = benchmark_step(
            Benchmark::ViewerRender,
            BenchmarkSelection::Standard {
                case: Some(BenchmarkCase::ViewerRenderRawArtifactSplitFull),
                sample_size: Some(20),
            },
            false,
            &["--save-baseline".to_owned(), "candidate".to_owned()],
        )
        .expect("viewer case belongs to viewer benchmark");

        assert_eq!(
            argument_strings(&step),
            [
                "bench",
                "-p",
                "gtl-benchmarks",
                "--bench",
                "viewer_render",
                "--",
                "raw-artifact-split-full",
                "--exact",
                "--sample-size",
                "20",
                "--save-baseline",
                "candidate"
            ]
        );
        assert_eq!(
            step.removed_environment(),
            ["DISPLAY".to_string(), "WAYLAND_DISPLAY".to_string()]
        );
    }

    #[test]
    fn benchmark_step_uses_the_catalogue_fast_cases() {
        let step = benchmark_step(
            Benchmark::ViewerRender,
            BenchmarkSelection::Fast,
            false,
            &[],
        )
        .expect("viewer benchmark has a fast preset");

        assert_eq!(
            argument_strings(&step),
            [
                "bench",
                "--quiet",
                "-p",
                "gtl-benchmarks",
                "--bench",
                "viewer_render",
                "--",
                "^(raw-artifact|raw-artifact-split-full)$",
                "--sample-size",
                "100",
                "--warm-up-time",
                "5",
                "--measurement-time",
                "5",
                "--discard-baseline",
                "--noplot",
                "--output-format",
                "bencher",
                "--color",
                "never"
            ]
        );
    }

    #[test]
    fn benchmark_step_rejects_a_case_from_another_target() {
        let result = benchmark_step(
            Benchmark::ViewCache,
            BenchmarkSelection::Standard {
                case: Some(BenchmarkCase::ViewerRenderRawArtifact),
                sample_size: None,
            },
            false,
            &[],
        );
        let Err(error) = result else {
            panic!("viewer case does not belong to view-cache benchmark");
        };

        assert_eq!(
            error.to_string(),
            "benchmark case raw-artifact belongs to viewer-render"
        );
    }

    #[test]
    fn benchmark_step_selects_the_parser_syntax_target_and_case() {
        let step = benchmark_step(
            Benchmark::ParserSyntax,
            BenchmarkSelection::Standard {
                case: Some(BenchmarkCase::ParserSyntaxRust45k),
                sample_size: None,
            },
            false,
            &[],
        )
        .expect("Rust syntax case belongs to parser-syntax benchmark");

        assert_eq!(
            argument_strings(&step),
            [
                "bench",
                "-p",
                "gtl-benchmarks",
                "--bench",
                "parser_syntax",
                "--",
                "parser-syntax/rust-45k",
                "--exact"
            ]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn benchmark_step_bounds_the_grpc_request_target_and_isolates_git() {
        let step = benchmark_step(
            Benchmark::GrpcRequests,
            BenchmarkSelection::Standard {
                case: None,
                sample_size: None,
            },
            true,
            &["--save-baseline".to_owned(), "grpc-requests-1".to_owned()],
        )
        .expect("bounded gRPC benchmark is supported on Linux");

        assert_eq!(step.program(), "/usr/bin/systemd-run");
        assert_eq!(
            argument_strings(&step),
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
                "--kill-after=10s",
                "15m",
                "cargo",
                "bench",
                "-p",
                "gtl-benchmarks",
                "--bench",
                "grpc_requests",
                "--",
                "--save-baseline",
                "grpc-requests-1"
            ]
        );
        assert_eq!(
            step.environment(),
            [
                ("CARGO_BUILD_JOBS".to_owned(), "1".to_owned()),
                ("RAYON_NUM_THREADS".to_owned(), "2".to_owned()),
                (
                    "GIT_AUTHOR_DATE".to_owned(),
                    "2026-01-01T00:00:00Z".to_owned()
                ),
                (
                    "GIT_COMMITTER_DATE".to_owned(),
                    "2026-01-01T00:00:00Z".to_owned()
                ),
                ("GIT_CONFIG_GLOBAL".to_owned(), "/dev/null".to_owned()),
                ("GIT_CONFIG_NOSYSTEM".to_owned(), "1".to_owned()),
                ("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()),
                ("LC_ALL".to_owned(), "C".to_owned()),
                ("TZ".to_owned(), "UTC".to_owned())
            ]
        );
    }
}
