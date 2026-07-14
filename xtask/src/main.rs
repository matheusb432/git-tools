//! `xtask` — this repo's embedded dev/release automation harness (ADR-0010).
//!
//! Invoked as `cargo run -p xtask -- <verb>` from the justfile; never installed (it is a
//! workspace member built on demand). Recipe bodies stay one-line forwarders into these verbs;
//! all automation *logic* lives here in Rust, not in shell. Add verbs to `cli::Command`.

use anyhow::Result;
use clap::Parser;

mod bench;
mod bootstrap;
mod build;
mod check_deps;
mod check_structure;
mod cli;
mod desktop_e2e;
mod drift;
mod fmt;
mod frontend;
mod icon;
mod install;
mod proc;
mod ship;
mod status_notifier;
mod testing;

fn main() {
    if let Err(e) = run(&cli::Cli::parse()) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

/// Dispatch one parsed verb to its handler.
fn run(cli: &cli::Cli) -> Result<()> {
    match &cli.command {
        cli::Command::Bootstrap => bootstrap::run(),
        cli::Command::Install { target } => install::run_install(*target),
        cli::Command::Uninstall {
            remove_config,
            force,
        } => install::run_uninstall(*remove_config, *force),
        cli::Command::Test { verbose, e2e, all } => testing::run(testing::TestOptions {
            verbose: *verbose,
            e2e: *e2e,
            all: *all,
        }),
        cli::Command::Build { target } => build::run(*target),
        cli::Command::FrontendTest => frontend::test(),
        cli::Command::DesktopBench => bench::run(),
        cli::Command::Fmt { check } => fmt::run(
            if *check {
                fmt::Action::Check
            } else {
                fmt::Action::Format
            },
            &[],
        ),
        cli::Command::Fix { extra } => fmt::run(fmt::Action::Fix, extra),
        cli::Command::DriftCheck => drift::run(),
        cli::Command::CheckStructure => check_structure::run(None),
        cli::Command::CheckDeps => check_deps::run(None),
        cli::Command::GenIcon => icon::run(),
        cli::Command::Ship { smoke, force } => ship::run(*smoke, *force),
    }
}
