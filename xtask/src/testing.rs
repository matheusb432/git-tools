//! `xtask test [--verbose] [--all]` — run the test suite. Migrates `just test` (the `ShellSpec`
//! install suite it used to also run is retired; its coverage is now `cargo test` Rust tests).
//! Terse by default (`cargo test --quiet`); `--verbose` streams full output; `--all` additionally
//! runs the bun frontend unit tests (`just cli test-js`) — the bun build/test stays bun, this only
//! orchestrates it.

use std::{ffi::OsStr, path::Path};

use anyhow::{Context, Result, anyhow, bail};

use crate::proc;

const DESKTOP_BUILD_ARGS: &[&str] = &[
    "build",
    "--release",
    "-p",
    "desktop",
    "--features",
    "custom-protocol",
];

/// Build the `cargo test` argv: quiet by default, streamed under `--verbose`.
fn cargo_test_args(verbose: bool) -> Vec<&'static str> {
    if verbose {
        vec!["test", "--", "--nocapture"]
    } else {
        vec!["test", "--quiet"]
    }
}

/// Run the suite. `cargo test` always; the bun frontend tests under `--all`.
pub fn run(verbose: bool, all: bool) -> Result<()> {
    proc::run("cargo-test", "cargo", &cargo_test_args(verbose))?;
    if all {
        proc::run("test-js", "just", &["cli", "test-js"])?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct E2eCommand {
    program: &'static str,
    args: Vec<&'static str>,
}

fn e2e_command(os: &str) -> Result<E2eCommand> {
    match os {
        "linux" => Ok(E2eCommand {
            program: "xvfb-run",
            args: vec!["-a", "bun", "run", "--cwd", "e2e/viewer", "test:e2e"],
        }),
        "windows" => Ok(E2eCommand {
            program: "bun",
            args: vec!["run", "--cwd", "e2e/viewer", "test:e2e"],
        }),
        unsupported => bail!(
            "desktop WebDriver e2e is not configured for {unsupported}; the external Tauri provider supports this project on Linux and Windows"
        ),
    }
}

fn require_tool(name: &str, hint: &str) -> Result<()> {
    which::which(name).with_context(|| format!("required tool `{name}` is missing; {hint}"))?;
    Ok(())
}

fn preflight_desktop_build(
    os: &str,
    probe_linux_headers: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if os == "linux" {
        probe_linux_headers().map_err(|error| {
            anyhow!(
                "webkit2gtk-4.1 development headers are required to build the reviewed viewer; dispatch config-provisioner to install them declaratively: {error:#}"
            )
        })?;
    }
    Ok(())
}

fn run_with_cleanup(
    run: impl FnOnce() -> Result<()>,
    cleanup: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let run = run();
    let cleanup = cleanup();
    match (run, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => Err(anyhow!(
            "{primary:#}; additionally, viewer e2e cleanup failed: {cleanup:#}"
        )),
    }
}

/// Builds and drives the real desktop binary through the platform `WebDriver`.
pub fn run_desktop_e2e() -> Result<()> {
    let result = run_desktop_e2e_workflow();
    match &result {
        Ok(()) => proc::result("desktop-e2e", "PASS"),
        Err(_) => proc::result_fail_step("desktop-e2e", "workflow"),
    }
    result
}

fn run_desktop_e2e_workflow() -> Result<()> {
    require_tool(
        "bun",
        "install Bun through the declarative host configuration",
    )?;
    require_tool(
        "tauri-driver",
        "install tauri-driver through the declarative host configuration",
    )?;

    let command = e2e_command(std::env::consts::OS)?;
    if std::env::consts::OS == "linux" {
        require_tool(
            "WebKitWebDriver",
            "install the webkit2gtk-driver system package through config-provisioner",
        )?;
        require_tool("xvfb-run", "install Xvfb through config-provisioner")?;
    }
    preflight_desktop_build(std::env::consts::OS, || {
        proc::run_captured_with_env(
            "viewer-e2e-webkit-headers",
            None,
            OsStr::new("pkg-config"),
            &["--exists", "webkit2gtk-4.1"],
            &[],
        )
    })?;

    proc::run_in(
        "viewer-e2e-install",
        "e2e/viewer",
        "bun",
        &["install", "--frozen-lockfile"],
    )?;
    proc::run("viewer-e2e-cli-build", "just", &["cli", "build"])?;
    proc::run("viewer-e2e-build", "cargo", DESKTOP_BUILD_ARGS)?;

    let data_root = tempfile::tempdir().context("create isolated viewer e2e data root")?;
    let cli = format!("target/release/git-tools{}", std::env::consts::EXE_SUFFIX);
    let runner_env = [("GTL_E2E_DATA_ROOT", data_root.path().as_os_str())];
    let cleanup_env = [("GIT_TOOLS_DATA_DIR", data_root.path().as_os_str())];
    run_with_cleanup(
        || {
            proc::run_captured_with_env(
                "viewer WebDriver e2e",
                None,
                OsStr::new(command.program),
                &command.args,
                &runner_env,
            )
        },
        || {
            proc::run_captured_with_env(
                "viewer e2e daemon cleanup",
                Some(Path::new(".")),
                OsStr::new(&cli),
                &["daemon", "stop"],
                &cleanup_env,
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn cargo_test_args_are_quiet_by_default() {
        assert_eq!(cargo_test_args(false), ["test", "--quiet"]);
    }

    #[test]
    fn cargo_test_args_stream_under_verbose() {
        assert_eq!(cargo_test_args(true), ["test", "--", "--nocapture"]);
    }

    #[test]
    fn linux_e2e_runs_bun_under_an_auto_numbered_xvfb_server() {
        assert_eq!(
            e2e_command("linux").unwrap(),
            E2eCommand {
                program: "xvfb-run",
                args: vec!["-a", "bun", "run", "--cwd", "e2e/viewer", "test:e2e"],
            }
        );
    }

    #[test]
    fn windows_e2e_does_not_require_an_x_server() {
        assert_eq!(
            e2e_command("windows").unwrap(),
            E2eCommand {
                program: "bun",
                args: vec!["run", "--cwd", "e2e/viewer", "test:e2e"],
            }
        );
    }

    #[test]
    fn unsupported_desktop_driver_platform_fails_explicitly() {
        assert!(e2e_command("macos").is_err());
    }

    #[test]
    fn linux_header_preflight_is_actionable() {
        let called = Cell::new(false);
        let error = preflight_desktop_build("linux", || {
            called.set(true);
            bail!("pkg-config exit 1")
        })
        .unwrap_err();

        assert!(called.get());
        assert!(error.to_string().contains("webkit2gtk-4.1"));
        assert!(error.to_string().contains("config-provisioner"));
    }

    #[test]
    fn non_linux_header_preflight_skips_pkg_config() {
        let called = Cell::new(false);
        preflight_desktop_build("windows", || {
            called.set(true);
            Ok(())
        })
        .unwrap();

        assert!(!called.get());
    }

    #[test]
    fn desktop_build_is_release_custom_protocol_and_cannot_soft_skip() {
        assert_eq!(
            DESKTOP_BUILD_ARGS,
            [
                "build",
                "--release",
                "-p",
                "desktop",
                "--features",
                "custom-protocol"
            ]
        );
    }

    #[test]
    fn runner_failure_still_executes_cleanup() {
        let cleaned = Cell::new(false);
        let error = run_with_cleanup(
            || bail!("runner failed"),
            || {
                cleaned.set(true);
                Ok(())
            },
        )
        .unwrap_err();

        assert!(cleaned.get());
        assert!(error.to_string().contains("runner failed"));
    }

    #[test]
    fn cleanup_failure_is_reported_without_masking_runner_failure() {
        let error =
            run_with_cleanup(|| bail!("runner failed"), || bail!("cleanup failed")).unwrap_err();

        let message = error.to_string();
        assert!(message.contains("runner failed"));
        assert!(message.contains("cleanup failed"));
    }

    #[test]
    fn cleanup_failure_is_reported_after_a_successful_runner() {
        let error = run_with_cleanup(|| Ok(()), || bail!("cleanup failed")).unwrap_err();

        assert!(error.to_string().contains("cleanup failed"));
    }
}
