//! Build and launch the bounded gRPC transport benchmark driver.

use std::path::Path;

use anyhow::{Result, bail};
use clap::Args;

use super::{cargo_target_directory, repository_root};
use crate::{process, task::Step};

#[derive(Args, Debug)]
pub(crate) struct GrpcTransportBenchmarkArguments {
    /// Compare the current result, then replace the local baseline after a successful run.
    #[arg(long)]
    pub(crate) update: bool,
}

#[derive(Clone, Copy)]
enum Mode {
    Benchmark { update: bool },
    Smoke,
}

pub(crate) fn run_benchmark(arguments: &GrpcTransportBenchmarkArguments) -> Result<()> {
    run(Mode::Benchmark {
        update: arguments.update,
    })
}

pub(crate) fn run_smoke() -> Result<()> {
    run(Mode::Smoke)
}

fn run(mode: Mode) -> Result<()> {
    require_linux_systemd()?;
    let root = repository_root();
    build_release_binaries(&root)?;
    let driver = cargo_target_directory(&root)?.join("release/grpc-transport-driver");
    let (driver_argument, wall_time_minutes) = match mode {
        Mode::Benchmark { update: true } => (Some("--update"), 30),
        Mode::Benchmark { update: false } => (None, 30),
        Mode::Smoke => (Some("--smoke"), 2),
    };
    process::run_step(&bounded_driver_step(
        &root,
        &driver,
        driver_argument,
        wall_time_minutes,
    ))
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
        .with_environment("CARGO_BUILD_JOBS", "1")
        .with_current_directory(root),
    )
}

fn bounded_driver_step(
    root: &Path,
    driver: &Path,
    driver_argument: Option<&str>,
    wall_time_minutes: u64,
) -> Step {
    let mut arguments = vec![
        "--user".to_owned(),
        "--scope".to_owned(),
        "--quiet".to_owned(),
        "--collect".to_owned(),
        "--property=CPUQuota=200%".to_owned(),
        "--property=MemoryMax=2147483648".to_owned(),
        "--property=MemorySwapMax=0".to_owned(),
        "--property=TasksMax=128".to_owned(),
        "/usr/bin/nice".to_owned(),
        "-n".to_owned(),
        "10".to_owned(),
        "/usr/bin/timeout".to_owned(),
        "--signal=TERM".to_owned(),
        "--kill-after=15s".to_owned(),
        format!("{wall_time_minutes}m"),
        driver.to_string_lossy().into_owned(),
    ];
    arguments.extend(driver_argument.map(str::to_owned));
    Step::new(
        "bounded production gRPC transport measurement",
        "/usr/bin/systemd-run",
        arguments,
    )
    .with_current_directory(root)
}

fn require_linux_systemd() -> Result<()> {
    if std::env::consts::OS != "linux" {
        bail!("bounded gRPC transport benchmark requires Linux systemd user scopes");
    }
    Ok(())
}
