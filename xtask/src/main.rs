//! `xtask` — this repo's embedded dev/release automation harness.
//!
//! Invoked as `cargo run -p xtask -- <verb>` from the justfile; never installed (it is a
//! workspace member built on demand). Direct quality-tool invocations stay in the justfile while
//! typed and nontrivial automation remains here. Add verbs under `verbs/` and wire the new arm
//! into `cli::Command` plus the dispatch below.

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
        cli::Command::Setup => verbs::setup::run(),
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
        cli::Command::Build { target } => verbs::build::run(target),
        cli::Command::WebBuild => verbs::dioxus_web::run(),
        cli::Command::WebServe { arguments } => verbs::dioxus_web::serve(&arguments),
        cli::Command::WebStyles => verbs::dioxus_web::run_styles(),
        cli::Command::Bench(arguments) => verbs::bench::run(&arguments),
        cli::Command::CheckDioxusFormat => verbs::format::check_dioxus(),
        cli::Command::PreCommit => verbs::pre_commit::run(),
        cli::Command::DriftCheck => verbs::drift::run(),
        cli::Command::CheckStructure => {
            verbs::check_structure::run(None);
            Ok(())
        }
        cli::Command::GenIcon => verbs::icon::run(),
        cli::Command::Ship { smoke, force } => verbs::ship::run(smoke, force),
    }
}
