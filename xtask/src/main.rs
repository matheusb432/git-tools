//! `xtask` — this repo's embedded dev/release automation harness.
//!
//! Invoked as `cargo run -p xtask -- <verb>` from the justfile; never installed (it is a
//! workspace member built on demand). Recipe bodies stay one-line forwarders into these verbs;
//! all automation *logic* lives here in Rust, not in shell. Add verbs under `verbs/` and wire the
//! new arm into `cli::Command` plus the dispatch below.

use anyhow::Result;
use clap::Parser;

mod cli;
mod process;
mod project;
mod task;
mod verb;
mod verbs;

fn main() {
    if let Err(e) = run(cli::Cli::parse().command) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

/// Dispatch one parsed verb to its focused workflow module.
fn run(command: cli::Command) -> Result<()> {
    match command {
        cli::Command::Bootstrap => verbs::bootstrap::run(),
        cli::Command::Install { target } => verbs::install::run_install(target),
        cli::Command::Uninstall {
            remove_config,
            force,
        } => verbs::install::run_uninstall(remove_config, force),
        cli::Command::Test(arguments) => verbs::test::run(&arguments),
        cli::Command::DesktopE2eWorker { verbose } => verbs::desktop_e2e::run_worker(verbose),
        cli::Command::E2eRuntimeWorker {
            executable,
            arguments,
        } => verbs::desktop_e2e::run_runtime(&executable, &arguments),
        cli::Command::Cov => verbs::cov::run(),
        cli::Command::Build { target } => verbs::build::run(target),
        cli::Command::FrontendTest => verbs::frontend::test(),
        cli::Command::FrontendBench => verbs::frontend::bench(),
        cli::Command::DesktopBench(arguments) => verbs::bench::run(&arguments),
        cli::Command::Fmt(arguments) => verbs::format::run(arguments.verbose),
        cli::Command::FmtCheck(arguments) => verbs::format::check(arguments.verbose),
        cli::Command::Lint => verbs::lint::run(),
        cli::Command::Check => verbs::check::run(),
        cli::Command::PreCommit => verbs::pre_commit::run(),
        cli::Command::Fix { extra } => verbs::fix::run(&extra),
        cli::Command::DriftCheck => verbs::drift::run(),
        cli::Command::CheckStructure => {
            verbs::check_structure::run(None);
            Ok(())
        }
        cli::Command::CheckDeps => {
            verbs::check_deps::run(None);
            Ok(())
        }
        cli::Command::GenIcon => verbs::icon::run(),
        cli::Command::Ship { smoke, force } => verbs::ship::run(smoke, force),
    }
}
