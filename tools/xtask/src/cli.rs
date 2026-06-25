//! Command-line surface for `xtask`. clap derives `--help` from these doc comments,
//! so they are the single source of truth for the verb documentation (ADR-0002). Add each
//! new automation verb here as a `Command` arm; let clap validate, don't hand-roll guards.

use clap::{Parser, Subcommand};

/// xtask — this repo's embedded dev/release automation (xtask).
#[derive(Parser)]
#[command(version, about = "this repo's embedded dev/release automation (xtask)")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// The verb surface. Each arm is one automation task its justfile recipe forwards into
/// (`cargo run -p xtask -- <verb>`). Express mutually-exclusive flags with clap's
/// `conflicts_with` (see the commented `--all` example), never a runtime `if a && b`.
#[derive(Subcommand)]
pub enum Command {
    /// One-time dev-host setup that runs *after* the toolchain exists (deps, installs, links).
    /// The toolchain install itself stays in `bootstrap.sh` — see that file.
    Bootstrap,
    /// Example read-only verb. Replace with real ones (`test`/`up`/`ship`/…). `--verbose`
    /// streams full tool logs instead of the terse default.
    Check {
        /// Stream full tool logs live instead of the terse default.
        #[arg(long)]
        verbose: bool,
        // Example of a mutually-exclusive flag — uncomment when a real verb needs it:
        // /// Run the full suite (mutually exclusive with `--e2e`).
        // #[arg(long, conflicts_with = "e2e")]
        // all: bool,
    },
}
