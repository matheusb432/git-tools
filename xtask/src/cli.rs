//! Command-line surface for `xtask`. clap derives `--help` from these doc comments,
//! so they are the single source of truth for the verb documentation (ADR-0002). Add each
//! new automation verb here as a `Command` arm; let clap validate, don't hand-roll guards.

use clap::{Parser, Subcommand, ValueEnum};

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
    /// Full post-toolchain dev-host bring-up: link `.claude/skills`, build + install both
    /// artifacts, ensure `~/.local/bin` is on PATH. Migrates `install-git-tools.sh` + the old
    /// skills-link recipe. The toolchain install itself stays in `bootstrap.sh` — see that file.
    Bootstrap,
    /// Place the prebuilt CLI engine (`git-tools` + `gtl` alias + `gtl-daemon`) and/or the
    /// desktop viewer on PATH. Builds are owned by the justfile; this only copies the
    /// already-built artifacts. Migrates `scripts/install.sh`.
    Install {
        /// Which artifact(s) to place: `cli`, `viewer`, or `both` (default).
        #[arg(long, value_enum, default_value_t = InstallTarget::Both)]
        target: InstallTarget,
    },
    /// Remove the installed CLI binary + `gtl` alias + `gtl-daemon` and the desktop viewer from
    /// PATH. Migrates `scripts/install.sh uninstall`.
    Uninstall {
        /// Also delete repo-local git-tools.toml / git-tools.secrets.toml (refused
        /// non-interactively unless `--force`).
        #[arg(long)]
        remove_config: bool,
        /// Proceed with the destructive config delete without prompting.
        #[arg(long)]
        force: bool,
    },
    /// Format Rust with the pinned-nightly rustfmt (toolchain from `.rustfmt-nightly`) and all
    /// TOML with taplo (skipped when absent). `--check` verifies without writing. Migrates
    /// `just fmt` / `just fmt-check`.
    Fmt {
        /// Verify formatting without writing (exits non-zero on drift).
        #[arg(long)]
        check: bool,
    },
    /// Run the test suite: `cargo test`, terse by default. `--verbose` streams full output;
    /// `--all` also runs the Deno frontend type-check and unit tests. Migrates `just test`.
    Test {
        /// Stream full test output (`cargo test -- --nocapture`) instead of the terse default.
        #[arg(long)]
        verbose: bool,
        /// Also run the Deno frontend type-check and unit tests (`just cli test-js`).
        #[arg(long)]
        all: bool,
    },
    /// Build and drive the real gtl-viewer binary through Tauri's external `WebDriver` provider.
    DesktopTestE2e,
    /// Rebuild the committed diff-preview bundle and fail if it drifts from its TypeScript
    /// sources. Requires Deno.
    /// Migrates the `_js-drift-guard` recipe — a CI/pre-commit gate.
    DriftCheck,
    /// Mechanical architecture lint: walks `crates/*/src` and `shared/*/src` and exits 3 on
    /// layout violations (max dir depth 2, flat feature folders, no `services/` dir).
    CheckStructure,
    /// Dependency-direction lint: exits 3 when `shared/*` depends on app crates or a core
    /// crate (`domain`/`application`/`contracts`) depends on outer crates/frameworks.
    CheckDeps,
    /// Render the gtl-viewer icon assets (`crates/desktop/icons/icon.{png,ico}`) from code.
    /// Ports the retired Python generator; the multi-res `.ico` is required by tauri-build on
    /// Windows.
    GenIcon,
    /// Cross-build the Win11 shippables (CLI + viewer + gtl-daemon) from this Linux host via
    /// cargo-xwin. `--smoke` is a fast debug-profile linkage check; the default is the release
    /// ship + verify.
    Ship {
        /// Debug-profile compile-smoke of all three binaries — a non-authoritative linkage drift
        /// check (no artifact verify), not a shippable.
        #[arg(long)]
        smoke: bool,
    },
}

/// Which artifact(s) `install` places. `both` covers the CLI engine and the desktop viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum InstallTarget {
    Cli,
    Viewer,
    Both,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Cli;

    #[test]
    fn ship_accepts_the_smoke_flag() {
        let cli = Cli::try_parse_from(["xtask", "ship", "--smoke"]).unwrap();
        assert!(matches!(cli.command, super::Command::Ship { smoke: true }));
    }

    #[test]
    fn desktop_test_e2e_is_a_closed_verb_without_flags() {
        let cli = Cli::try_parse_from(["xtask", "desktop-test-e2e"]).unwrap();
        assert!(matches!(cli.command, super::Command::DesktopTestE2e));
        assert!(Cli::try_parse_from(["xtask", "desktop-test-e2e", "--raw"]).is_err());
    }
}
