//! Headless viewer-render benchmark entrypoint.

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::{process, task::Step};

const DEFAULT_SAMPLE_SIZE: usize = 10;
const MINIMUM_SAMPLE_SIZE: usize = 10;

#[derive(Args, Debug)]
pub(crate) struct DesktopBenchArguments {
    /// Benchmark target to run.
    #[arg(long, value_enum, default_value_t = Benchmark::ViewerRender)]
    benchmark: Benchmark,
    /// Number of Criterion samples collected for each benchmark.
    #[arg(
        long,
        default_value_t = DEFAULT_SAMPLE_SIZE,
        value_parser = parse_sample_size
    )]
    sample_size: usize,
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
    process::run_step(&benchmark_step(arguments.benchmark, arguments.sample_size))
}

fn benchmark_step(benchmark: Benchmark, sample_size: usize) -> Step {
    let arguments = [
        "bench",
        "-p",
        "desktop",
        "--bench",
        benchmark.cargo_target(),
        "--features",
        "benchmark-support",
        "--",
        "--sample-size",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([sample_size.to_string()])
    .collect::<Vec<_>>();

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
    fn benchmark_step_forwards_typed_target_and_sample_size() {
        let step = benchmark_step(Benchmark::ViewerRender, 10);

        assert_eq!(
            argument_strings(&step),
            [
                "bench",
                "-p",
                "desktop",
                "--bench",
                "viewer_render",
                "--features",
                "benchmark-support",
                "--",
                "--sample-size",
                "10"
            ]
        );
        assert_eq!(
            step.removed_environment(),
            ["DISPLAY".to_string(), "WAYLAND_DISPLAY".to_string()]
        );
    }
}
