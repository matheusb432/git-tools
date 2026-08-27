use std::{ffi::OsString, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::verbs::{
    Verb, desktop_scroll::DesktopScrollBenchmarkArguments,
    grpc_transport::GrpcTransportBenchmarkArguments,
    server_highlighting::ServerHighlightingBenchmarkArguments, test::TestArguments,
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
    /// Build and install both artifacts, and ensure `~/.local/bin` is on PATH.
    #[command(name = Verb::SETUP.as_str())]
    Setup,
    /// Place the prebuilt CLI engine (`git-tools` + `gtl` alias + `gtl-server`) and/or the
    /// desktop viewer on PATH. Builds are owned by the justfile; this only copies the
    /// already-built artifacts.
    #[command(name = Verb::INSTALL.as_str())]
    Install {
        /// Which artifact(s) to place: `cli`, `viewer`, or `both` (default).
        #[arg(long, value_enum, default_value_t = InstallTarget::Both)]
        target: InstallTarget,
    },
    /// Remove the installed CLI binary + `gtl` alias + `gtl-server` and the desktop viewer from
    /// PATH.
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
    /// Validate Dioxus RSX formatting without allowing the formatter to rewrite source files.
    #[command(name = Verb::CHECK_DIOXUS_FORMAT.as_str(), hide = true)]
    CheckDioxusFormat,
    /// Check staged whitespace and formatting without scanning unrelated files.
    #[command(name = Verb::PRE_COMMIT.as_str())]
    PreCommit,
    /// Run the selected test scope, terse by default. The default excludes desktop tests;
    /// `--e2e` is hermetic viewer E2E only; `--all` is the complete repository test suite.
    #[command(name = Verb::TEST.as_str())]
    Test(TestArguments),
    /// Run the ordered native desktop E2E workflow for the test supervisor.
    #[command(hide = true)]
    DesktopE2eWorker {
        /// Preserve the worker's live diagnostics.
        #[arg(long)]
        verbose: bool,
    },
    /// Regenerate and verify the committed deterministic desktop scroll fixture.
    #[command(name = Verb::DESKTOP_SCROLL_FIXTURE.as_str())]
    DesktopScrollFixture,
    /// Measure production desktop scrolling against the committed realistic fixture.
    #[command(name = Verb::DESKTOP_SCROLL_BENCHMARK.as_str())]
    DesktopScrollBenchmark(DesktopScrollBenchmarkArguments),
    /// Measure production server-owned syntax highlighting and compare it with the local baseline.
    #[command(name = Verb::SERVER_HIGHLIGHTING_BENCHMARK.as_str())]
    ServerHighlightingBenchmark(ServerHighlightingBenchmarkArguments),
    /// Measure the release gRPC transport with ghz and compare it with the local baseline.
    #[command(name = Verb::GRPC_TRANSPORT_BENCHMARK.as_str())]
    GrpcTransportBenchmark(GrpcTransportBenchmarkArguments),
    /// Validate the release gRPC transport with a short, non-comparable ghz workload.
    #[command(name = Verb::GRPC_TRANSPORT_SMOKE.as_str())]
    GrpcTransportSmoke,
    /// Attribute full-language server highlighting allocations with Valgrind Massif.
    #[command(name = Verb::SERVER_HIGHLIGHTING_PROFILE.as_str())]
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
    #[command(name = Verb::BUILD.as_str())]
    Build {
        /// Which release artifact set to build.
        #[arg(long, value_enum, default_value_t = BuildTarget::Both)]
        target: BuildTarget,
    },
    /// Generate the static artifact stylesheet and stage the release Dioxus Web bundle.
    #[command(name = Verb::WEB_BUILD.as_str())]
    WebBuild,
    /// Serve the Dioxus shell with repository-owned asset watchers.
    #[command(name = Verb::WEB_SERVE.as_str())]
    WebServe {
        /// Arguments forwarded to `dx serve`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<String>,
    },
    /// Generate the tracked shared Tailwind stylesheet.
    #[command(name = Verb::WEB_STYLES.as_str())]
    WebStyles,
    /// Rebuild web assets and fail if the tracked stylesheet drifts from its sources.
    #[command(name = Verb::DRIFT_CHECK.as_str())]
    DriftCheck,
    /// Reject forbidden outward Cargo dependency edges.
    #[command(name = Verb::CHECK_STRUCTURE.as_str())]
    CheckStructure,
    /// Compile every parser feature for the browser target with the managed C toolchain.
    #[command(name = Verb::CHECK_PARSER_WASM.as_str(), hide = true)]
    CheckParserWasm,
    /// Render the gtl-viewer icon assets (`crates/gtl-desktop/icons/icon.{png,ico}`) from code.
    #[command(name = Verb::GEN_ICON.as_str())]
    GenIcon,
    /// Cross-build the Win11 shippables (CLI + viewer + gtl-server) from this Linux host via
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
