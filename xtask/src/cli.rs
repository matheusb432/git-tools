//! Command-line surface for `xtask`. clap derives `--help` from these doc comments,
//! so they are the single source of truth for the verb documentation. Add each
//! new automation verb here as a `Command` arm; let clap validate, don't hand-roll guards.

use std::{ffi::OsString, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::{
    verb::Verb,
    verbs::{bench::DesktopBenchArguments, format::FormatArguments, test::TestArguments},
};

/// xtask — this repo's embedded dev/release automation (xtask).
#[derive(Parser)]
#[command(
    version,
    about = "this repo's embedded dev/release automation (xtask)",
    styles = clap_cargo::style::CLAP_STYLING
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// The verb surface. Each arm is one automation task its justfile recipe forwards into
/// (`cargo run -p xtask -- <verb>`). Express mutually-exclusive flags with clap's
/// `conflicts_with` (see the commented `--all` example), never a runtime `if a && b`.
#[derive(Subcommand)]
pub enum Command {
    /// Repository-local bootstrap phase: configure hooks, install frontend dependencies, build
    /// and install both artifacts, and ensure `~/.local/bin` is on PATH. Mise owns host packages,
    /// toolchains, and shell activation.
    #[command(name = Verb::BOOTSTRAP.as_str())]
    Bootstrap,
    /// Place the prebuilt CLI engine (`git-tools` + `gtl` alias + `gtl-daemon`) and/or the
    /// desktop viewer on PATH. Builds are owned by the justfile; this only copies the
    /// already-built artifacts. Migrates `scripts/install.sh`.
    #[command(name = Verb::INSTALL.as_str())]
    Install {
        /// Which artifact(s) to place: `cli`, `viewer`, or `both` (default).
        #[arg(long, value_enum, default_value_t = InstallTarget::Both)]
        target: InstallTarget,
    },
    /// Remove the installed CLI binary + `gtl` alias + `gtl-daemon` and the desktop viewer from
    /// PATH. Migrates `scripts/install.sh uninstall`.
    #[command(name = Verb::UNINSTALL.as_str())]
    Uninstall {
        /// Also delete repo-local git-tools.toml / git-tools.secrets.toml (refused
        /// non-interactively unless `--force`).
        #[arg(long)]
        remove_config: bool,
        /// Proceed with the destructive config delete without prompting.
        #[arg(long)]
        force: bool,
    },
    /// Format Rust, TOML, Markdown, and frontend sources with the repository's complete pinned
    /// formatter matrix, in place. `--verbose` restores taplo's file-discovery logs.
    #[command(name = Verb::FORMAT.as_str())]
    Fmt(FormatArguments),
    /// Check formatting without modifying files; exits non-zero on drift. Formatting only — the
    /// linters live under `lint` and the aggregate `check` gate. `--verbose` restores taplo's
    /// file-discovery logs.
    #[command(name = Verb::FORMAT_CHECK.as_str())]
    FmtCheck(FormatArguments),
    /// Run Oxlint, presentation and architecture policy, dependency checks, and workspace Clippy.
    #[command(name = Verb::LINT.as_str())]
    Lint,
    /// Run formatting checks, the lint sweep, and configured ast-grep rules.
    #[command(name = Verb::CHECK.as_str())]
    Check,
    /// Check staged whitespace, formatting, and frontend lint without scanning unrelated files.
    #[command(name = Verb::PRE_COMMIT.as_str())]
    PreCommit,
    /// Apply autofixable Rust and frontend lints, then run every configured formatter.
    #[command(name = Verb::FIX.as_str())]
    Fix {
        /// Extra arguments forwarded to Cargo Clippy.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// Run the selected test scope, terse by default. The default excludes desktop tests;
    /// `--e2e` is hermetic viewer E2E only; `--all` is the complete repository gate.
    #[command(name = Verb::TEST.as_str())]
    Test(TestArguments),
    /// Run the ordered native desktop E2E workflow for the test supervisor.
    #[command(hide = true)]
    DesktopE2eWorker {
        /// Preserve the worker's live diagnostics.
        #[arg(long)]
        verbose: bool,
    },
    /// Launch one compiled E2E executable without the host-only Cargo environment.
    #[command(hide = true)]
    E2eRuntimeWorker {
        /// Compiled test or support executable selected by Cargo.
        executable: PathBuf,
        /// Arguments Cargo forwards to the executable.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
    /// Build the CLI engine, desktop viewer, or both release artifacts.
    #[command(name = Verb::BUILD.as_str())]
    Build {
        /// Which release artifact set to build.
        #[arg(long, value_enum, default_value_t = BuildTarget::Both)]
        target: BuildTarget,
    },
    /// Type-check and test the framework-free frontend sources.
    #[command(name = Verb::FRONTEND_TEST.as_str())]
    FrontendTest,
    /// Run the frontend compute benchmarks (tinybench through vitest bench).
    #[command(name = Verb::FRONTEND_BENCH.as_str())]
    FrontendBench,
    /// Run the pure viewer-render benchmark without a host display.
    #[command(name = Verb::DESKTOP_BENCH.as_str())]
    DesktopBench(DesktopBenchArguments),
    /// Rebuild the committed diff-preview bundle and fail if it drifts from its TypeScript
    /// sources. Requires Deno.
    #[command(name = Verb::DRIFT_CHECK.as_str())]
    DriftCheck,
    /// Reject forbidden outward Cargo dependency edges.
    #[command(name = Verb::CHECK_STRUCTURE.as_str())]
    CheckStructure,
    /// Render the gtl-viewer icon assets (`crates/desktop/icons/icon.{png,ico}`) from code.
    /// Ports the retired Python generator; the multi-res `.ico` is required by tauri-build on
    /// Windows.
    #[command(name = Verb::GEN_ICON.as_str())]
    GenIcon,
    /// Cross-build the Win11 shippables (CLI + viewer + gtl-daemon) from this Linux host via
    /// cargo-xwin. `--smoke` is a fast debug-profile linkage check; the default is the release
    /// ship + verify.
    #[command(name = Verb::SHIP.as_str())]
    Ship {
        /// Debug-profile compile-smoke of all three binaries — a non-authoritative linkage drift
        /// check (no artifact verify), not a shippable.
        #[arg(long)]
        smoke: bool,
        /// Skip only the `just test --all` preflight.
        #[arg(short = 'f', long)]
        force: bool,
    },
}

/// Which release artifact set `build` produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BuildTarget {
    Cli,
    Viewer,
    Both,
}

/// Which artifact(s) `install` places. `both` covers the CLI engine and the desktop viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum InstallTarget {
    Cli,
    Viewer,
    Both,
}
