//! Headless viewer-render benchmark entrypoint.

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::{process, task::Step};

const DEFAULT_SAMPLE_SIZE: usize = 10;
const FAST_SAMPLE_SIZE: usize = 100;
const FAST_WARM_UP_SECONDS: u64 = 5;
const FAST_MEASUREMENT_SECONDS: u64 = 5;
const FAST_CASE_FILTER: &str = "^materialized-shell-(45k|115-files)$";
const MINIMUM_SAMPLE_SIZE: usize = 10;

#[derive(Args, Debug)]
pub(crate) struct DesktopBenchArguments {
    /// Benchmark target to run.
    #[arg(long, value_enum, default_value_t = Benchmark::ViewerRender)]
    benchmark: Benchmark,
    /// Run the two shell-only cases with a stable, concise Criterion preset.
    #[arg(long, conflicts_with_all = ["case", "sample_size"])]
    fast: bool,
    /// Run exactly one viewer-render case.
    #[arg(long, value_enum, conflicts_with = "fast")]
    case: Option<ViewerRenderCase>,
    /// Criterion samples for full or selected runs; defaults to 10.
    #[arg(long, value_parser = parse_sample_size, conflicts_with = "fast")]
    sample_size: Option<usize>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Benchmark {
    #[value(name = "viewer-render")]
    ViewerRender,
}

impl Benchmark {
    const fn cargo_target(self) -> &'static str {
        match self {
            Self::ViewerRender => "viewer_render",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ViewerRenderCase {
    #[value(name = "unified-compact")]
    UnifiedCompact,
    #[value(name = "split-full")]
    SplitFull,
    #[value(name = "raw-artifact")]
    RawArtifact,
    #[value(name = "materialized-shell-45k")]
    MaterializedShell45k,
    #[value(name = "materialized-chunks-45k")]
    MaterializedChunks45k,
    #[value(name = "materialized-shell-115-files")]
    MaterializedShell115Files,
    #[value(name = "materialized-chunks-115-files")]
    MaterializedChunks115Files,
}

impl ViewerRenderCase {
    const fn criterion_filter(self) -> &'static str {
        match self {
            Self::UnifiedCompact => "unified-compact",
            Self::SplitFull => "split-full",
            Self::RawArtifact => "raw-artifact",
            Self::MaterializedShell45k => "materialized-shell-45k",
            Self::MaterializedChunks45k => "materialized-chunks-45k",
            Self::MaterializedShell115Files => "materialized-shell-115-files",
            Self::MaterializedChunks115Files => "materialized-chunks-115-files",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BenchmarkSelection {
    Standard {
        case: Option<ViewerRenderCase>,
        sample_size: usize,
    },
    Fast,
}

impl DesktopBenchArguments {
    fn selection(&self) -> BenchmarkSelection {
        if self.fast {
            BenchmarkSelection::Fast
        } else {
            BenchmarkSelection::Standard {
                case: self.case,
                sample_size: self.sample_size.unwrap_or(DEFAULT_SAMPLE_SIZE),
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

/// Run the pure renderer benchmark with host display variables removed.
pub(crate) fn run(arguments: &DesktopBenchArguments) -> Result<()> {
    process::run_step(&benchmark_step(arguments.benchmark, arguments.selection()))
}

fn benchmark_step(benchmark: Benchmark, selection: BenchmarkSelection) -> Step {
    let mut arguments = vec!["bench".to_owned()];
    if matches!(selection, BenchmarkSelection::Fast) {
        arguments.push("--quiet".to_owned());
    }
    arguments.extend(
        [
            "-p",
            "gtl-desktop",
            "--bench",
            benchmark.cargo_target(),
            "--features",
            "benchmark-support",
            "--",
        ]
        .into_iter()
        .map(str::to_owned),
    );

    match selection {
        BenchmarkSelection::Standard { case, sample_size } => {
            if let Some(case) = case {
                arguments.extend([case.criterion_filter().to_owned(), "--exact".to_owned()]);
            }
            arguments.extend(["--sample-size".to_owned(), sample_size.to_string()]);
        }
        BenchmarkSelection::Fast => {
            arguments.extend([
                FAST_CASE_FILTER.to_owned(),
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

    Step::new("desktop-render-benchmark", "cargo", arguments)
        .without_environment(["DISPLAY", "WAYLAND_DISPLAY"])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argument_strings(step: &Step) -> Vec<&str> {
        step.arguments().iter().map(String::as_str).collect()
    }

    #[test]
    fn benchmark_step_forwards_selected_case_and_sample_size() {
        let step = benchmark_step(
            Benchmark::ViewerRender,
            BenchmarkSelection::Standard {
                case: Some(ViewerRenderCase::MaterializedShell45k),
                sample_size: 20,
            },
        );

        assert_eq!(
            argument_strings(&step),
            [
                "bench",
                "-p",
                "gtl-desktop",
                "--bench",
                "viewer_render",
                "--features",
                "benchmark-support",
                "--",
                "materialized-shell-45k",
                "--exact",
                "--sample-size",
                "20"
            ]
        );
        assert_eq!(
            step.removed_environment(),
            ["DISPLAY".to_string(), "WAYLAND_DISPLAY".to_string()]
        );
    }

    #[test]
    fn benchmark_step_uses_the_criterion_fast_preset() {
        let step = benchmark_step(Benchmark::ViewerRender, BenchmarkSelection::Fast);

        assert_eq!(
            argument_strings(&step),
            [
                "bench",
                "--quiet",
                "-p",
                "gtl-desktop",
                "--bench",
                "viewer_render",
                "--features",
                "benchmark-support",
                "--",
                "^materialized-shell-(45k|115-files)$",
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
}
