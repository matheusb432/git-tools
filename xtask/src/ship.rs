//! `xtask ship` — cross-build the Win11 shippables (CLI + viewer + gtl-daemon) from this Linux
//! host via cargo-xwin. Migrates `scripts/win-preflight.sh` + the `win-build`/`win-compile-smoke`
//! recipes. Stages run side-effect-free-first and emit the `RESULT scope=ship …` contract; a
//! cross-build proves linkage, NOT runtime (host/release split — certify on real Win11).

use std::{path::Path, process::Command};

use anyhow::{Result, bail};

use crate::proc;

/// The Windows cross-target. `--features custom-protocol` is required for the viewer (else the
/// exe serves devUrl and fails with `ERR_CONNECTION_REFUSED`).
const WIN_TARGET: &str = "x86_64-pc-windows-msvc";

/// Whether the Linux→Windows cross toolchain is ready, plus fix-hint lines when not.
pub struct PreflightReport {
    pub ok: bool,
    pub lines: Vec<String>,
}

/// Decide whether the cross toolchain is ready. Pure: `have(tool)` probes PATH, `has_target`
/// probes installed rustup targets. The injected-closure seam replaces the shell guards.
pub fn preflight(
    have: &dyn Fn(&str) -> bool,
    has_target: &dyn Fn(&str) -> bool,
) -> PreflightReport {
    let mut lines = Vec::new();
    let mut ok = true;
    if !have("cargo-xwin") {
        ok = false;
        lines.push("cargo-xwin not found — run: cargo install cargo-xwin --locked".to_string());
    }
    if !has_target(WIN_TARGET) {
        ok = false;
        lines.push(format!(
            "rustup target '{WIN_TARGET}' missing — run: rustup target add {WIN_TARGET}"
        ));
    }
    PreflightReport { ok, lines }
}

/// Whether `tool` resolves to an executable on PATH. Uses the `which` crate — no shell, no
/// string interpolation, so there is no `sh -c` injection surface and it is cross-platform.
fn on_path(tool: &str) -> bool {
    which::which(tool).is_ok()
}

/// Whether `rustup target list --installed` contains `triple`.
fn target_installed(triple: &str) -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .is_ok_and(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim() == triple)
        })
}

/// Cross-build the Win11 shippables. `smoke` = fast debug linkage check (committed bundles, no
/// artifact verify); otherwise a release ship (fresh bundles + verify). Migrates the
/// `win-build`/`win-compile-smoke` recipes + `scripts/win-preflight.sh`.
pub fn run(smoke: bool) -> Result<()> {
    // 1. preflight (side-effect-free decision)
    let report = preflight(&on_path, &target_installed);
    if !report.ok {
        // ! Hints → stderr (human); the RESULT line → stdout (machine-parsed). Keep the split.
        for line in &report.lines {
            eprintln!("{line}");
        }
        proc::result_fail_step("ship", "preflight");
        bail!("ship preflight failed");
    }

    // 2. frontend bundle (release only — smoke uses the committed bundle for speed)
    if !smoke && proc::run("frontend-cli", "just", &["cli", "build-js"]).is_err() {
        proc::result_fail_step("ship", "frontend");
        bail!("ship frontend bundle build failed");
    }

    // 3. cross-build all three packages
    let profile: &[&str] = if smoke { &[] } else { &["--release"] };
    let mut cli_args = vec!["xwin", "build"];
    cli_args.extend_from_slice(profile);
    cli_args.extend_from_slice(&["-p", "cli", "--target", WIN_TARGET]);

    let mut daemon_args = vec!["xwin", "build"];
    daemon_args.extend_from_slice(profile);
    daemon_args.extend_from_slice(&["-p", "daemon", "--target", WIN_TARGET]);

    let mut viewer_args = vec!["xwin", "build"];
    viewer_args.extend_from_slice(profile);
    viewer_args.extend_from_slice(&[
        "-p",
        "desktop",
        "--features",
        "custom-protocol",
        "--target",
        WIN_TARGET,
    ]);

    if proc::run("cross-build-cli", "cargo", &cli_args).is_err()
        || proc::run("cross-build-daemon", "cargo", &daemon_args).is_err()
        || proc::run("cross-build-viewer", "cargo", &viewer_args).is_err()
    {
        proc::result_fail_step("ship", "cross-build");
        bail!("ship cross-build failed");
    }

    // 4. verify artifacts (release only)
    if !smoke {
        for exe in ["git-tools.exe", "gtl-daemon.exe", "gtl-viewer.exe"] {
            let path = Path::new("target")
                .join(WIN_TARGET)
                .join("release")
                .join(exe);
            let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
            if bytes == 0 {
                proc::result_fail_step("ship", "verify");
                bail!("ship verify: {} is missing or empty", path.display());
            }
        }
    }

    proc::result("ship", "PASS");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_fails_with_install_hint_when_cargo_xwin_missing() {
        let r = preflight(&|t| t != "cargo-xwin", &|_| true);
        assert!(!r.ok);
        assert!(
            r.lines
                .iter()
                .any(|l| l.contains("cargo install cargo-xwin --locked")),
            "expected install hint, got {:?}",
            r.lines
        );
    }

    #[test]
    fn preflight_fails_with_target_hint_when_target_missing() {
        let r = preflight(&|_| true, &|tr| tr != WIN_TARGET);
        assert!(!r.ok);
        assert!(
            r.lines
                .iter()
                .any(|l| l.contains("rustup target add x86_64-pc-windows-msvc")),
            "expected target hint, got {:?}",
            r.lines
        );
    }

    #[test]
    fn preflight_ok_when_both_present() {
        assert!(preflight(&|_| true, &|_| true).ok);
    }
}
