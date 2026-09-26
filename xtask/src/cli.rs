use std::{ffi::OsString, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::verbs::{
    desktop_scroll::DesktopScrollBenchmarkArguments,
    server_highlighting::ServerHighlightingBenchmarkArguments,
    view_source::ViewSourceBenchmarkArguments,
};

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

#[derive(Subcommand)]
pub enum Command {
    /// Add the configured install directory to the Zsh PATH when needed.
    EnsureInstallPath,
    /// Install prebuilt CLI, server, or viewer artifacts.
    Install {
        /// Which artifact(s) to place: `cli`, `viewer`, or `both` (default).
        #[arg(long, value_enum, default_value_t = InstallTarget::Both)]
        target: InstallTarget,
    },
    /// Remove installed CLI, server, and viewer artifacts.
    Uninstall {
        /// Also delete repo-local git-tools.toml / git-tools.secrets.toml (refused
        /// non-interactively unless `--force`).
        #[arg(long)]
        remove_config: bool,
        /// Proceed with the destructive config delete without prompting.
        #[arg(long)]
        force: bool,
    },
    /// Validate Dioxus RSX formatting without allowing the formatter to rewrite source files.
    #[command(hide = true)]
    CheckDioxusFormat,
    /// Check staged whitespace and formatting without scanning unrelated files.
    PreCommit,
    /// Run one hermetic desktop E2E suite for the test supervisor.
    #[command(hide = true)]
    DesktopE2eWorker {
        /// Suite to build and run.
        #[arg(value_enum)]
        suite: DesktopE2eSuite,
        /// Nextest arguments that select journeys; only the journey suite accepts them.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        nextest_arguments: Vec<OsString>,
    },
    /// Regenerate and verify the committed deterministic desktop scroll fixture.
    DesktopScrollFixture,
    /// Measure production desktop scrolling against the committed realistic fixture.
    DesktopScrollBenchmark(DesktopScrollBenchmarkArguments),
    /// Measure production server-owned syntax highlighting and compare it with the local baseline.
    ServerHighlightingBenchmark(ServerHighlightingBenchmarkArguments),
    /// Measure Compact construction and the first demand-loaded Full transition.
    ViewSourceBenchmark(ViewSourceBenchmarkArguments),
    /// Attribute full-language server highlighting allocations with Valgrind Massif.
    ServerHighlightingProfile,
    /// Regenerate the desktop scroll fixture inside the bounded worker scope.
    #[command(hide = true)]
    DesktopScrollFixtureWorker,
    /// Run the production desktop scroll journey inside the bounded worker scope.
    #[command(hide = true)]
    DesktopScrollBenchmarkWorker,
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
    Build {
        /// Which release artifact set to build.
        #[arg(long, value_enum, default_value_t = BuildTarget::Both)]
        target: BuildTarget,
    },
    /// Build the CLI, server, and desktop app and package a native macOS installer.
    MacosPackage,
    /// Exercise the installed macOS package and save a desktop launch screenshot.
    MacosSmoke,
    /// Generate the viewer stylesheet and stage the release Dioxus Web bundle.
    WebBuild,
    /// Serve the Dioxus shell with repository-owned asset watchers.
    WebServe {
        /// Arguments forwarded to `dx serve`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<String>,
    },
    /// Generate the tracked shared Tailwind stylesheet.
    WebStyles,
    /// Reject forbidden outward Cargo dependency edges.
    CheckStructure,
    /// Compile every parser feature for the browser target with the managed C toolchain.
    #[command(hide = true)]
    CheckParserWasm,
    /// Render the viewer's launcher, favicon, bundle, and per-theme tray and launcher icons.
    GenIcon,
    /// Cross-build Windows CLI, viewer, and server artifacts with cargo-xwin.
    Ship {
        /// Compile all binaries in debug mode without artifact verification.
        #[arg(long)]
        smoke: bool,
        /// Skip only the `just test-all` preflight.
        #[arg(short = 'f', long)]
        force: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BuildTarget {
    Cli,
    Viewer,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum DesktopE2eSuite {
    /// `WebDriver` journeys through the release Tauri viewer.
    Journeys,
    /// Native window focus across CLI diff opens.
    Native,
    /// Playwright raw-artifact and component-preview journeys.
    Browser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum InstallTarget {
    Cli,
    Viewer,
    Both,
}
