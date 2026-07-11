//! `xtask` — this repo's embedded dev/release automation harness (ADR-0010).
//!
//! Invoked as `cargo run -p xtask -- <verb>` from the justfile; never installed (it is a
//! workspace member built on demand). Recipe bodies stay one-line forwarders into these verbs;
//! all automation *logic* lives here in Rust, not in shell. Add verbs to `cli::Command`.

use anyhow::Result;
use clap::Parser;

mod bootstrap;
mod check_deps;
mod check_structure;
mod cli;
mod drift;
mod fmt;
mod icon;
mod install;
mod proc;
mod ship;
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
        cli::Command::Test { verbose, all } => testing::run(*verbose, *all),
        cli::Command::Fmt { check } => fmt::run(*check),
        cli::Command::DriftCheck => drift::run(),
        cli::Command::CheckStructure => check_structure::run(None),
        cli::Command::CheckDeps => check_deps::run(None),
        cli::Command::GenIcon => icon::run(),
        cli::Command::Ship { smoke } => ship::run(*smoke),
    }
}
