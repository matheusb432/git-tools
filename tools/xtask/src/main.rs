//! `xtask` — this repo's embedded dev/release automation harness (ADR-0010).
//!
//! Invoked as `cargo run -p xtask -- <verb>` from the justfile; never installed (it is a
//! workspace member built on demand). Recipe bodies stay one-line forwarders into these verbs;
//! all automation *logic* lives here in Rust, not in shell. Add verbs to `cli::Command`.

use anyhow::Result;
use clap::Parser;

mod cli;
mod icon;
mod proc;

fn main() {
    if let Err(e) = run(cli::Cli::parse()) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

/// Dispatch one parsed verb to its handler.
fn run(cli: cli::Cli) -> Result<()> {
    match cli.command {
        cli::Command::Bootstrap => bootstrap(),
        cli::Command::Check { verbose } => check(verbose),
        cli::Command::GenIcon => icon::run(),
    }
}

/// One-time dev-host setup that runs *after* the toolchain exists (deps, installs, links).
/// The toolchain install itself stays in `bootstrap.sh` (chicken-and-egg — see that file).
fn bootstrap() -> Result<()> {
    // TODO: replace with the real bring-up steps (e.g. `npm install`, fixtures, links).
    proc::run("noop", "true", &[])?;
    proc::result("bootstrap", "PASS");
    Ok(())
}

/// Example read-only verb. Replace with a real one (e.g. `test`/`up`/`ship`); model
/// mutually-exclusive flags with clap's `conflicts_with`, not a runtime guard.
fn check(verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("running check (verbose)…");
    }
    proc::run("check", "true", &[])?;
    proc::result("check", "PASS");
    Ok(())
}
