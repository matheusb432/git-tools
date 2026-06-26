//! `xtask fmt [--check]` — format (or check) Rust with the pinned-nightly rustfmt and all TOML
//! with taplo. Migrates `just fmt` / `just fmt-check`. The nightly toolchain is read from
//! `.rustfmt-nightly` (stable `cargo fmt` silently skips this repo's nightly-only rustfmt.toml
//! keys, so the `+toolchain` token is mandatory — see rust-style). taplo is optional: skipped
//! with a message when absent, never a hard failure.

use std::{env, fs};

use anyhow::{Context, Result};

use crate::proc;

/// Build the `cargo +<toolchain> fmt [--check]` argv. The `+toolchain` token must come first so
/// the rustup proxy selects the pinned nightly before `fmt` runs.
fn cargo_fmt_args(toolchain: &str, check: bool) -> Vec<String> {
    let mut args = vec![format!("+{toolchain}"), "fmt".to_string()];
    if check {
        args.push("--check".to_string());
    }
    args
}

/// Build the taplo argv: `fmt` (write) or `fmt --check` (verify).
fn taplo_args(check: bool) -> Vec<&'static str> {
    if check {
        vec!["fmt", "--check"]
    } else {
        vec!["fmt"]
    }
}

/// Read the pinned rustfmt nightly toolchain name from `.rustfmt-nightly` (repo root = CWD).
fn read_nightly() -> Result<String> {
    let path = env::current_dir()?.join(".rustfmt-nightly");
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("reading {} for the pinned rustfmt nightly", path.display()))?;
    Ok(raw.trim().to_string())
}

/// Format or check the workspace. Rust via pinned-nightly rustfmt (hard requirement); TOML via
/// taplo when present.
pub fn run(check: bool) -> Result<()> {
    let toolchain = read_nightly()?;
    let cargo_args = cargo_fmt_args(&toolchain, check);
    let cargo_args: Vec<&str> = cargo_args.iter().map(String::as_str).collect();
    proc::run("cargo-fmt", "cargo", &cargo_args)?;

    if which::which("taplo").is_ok() {
        proc::run("taplo", "taplo", &taplo_args(check))?;
    } else {
        eprintln!("taplo not installed; skipping");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_fmt_args_writes_by_default() {
        assert_eq!(
            cargo_fmt_args("nightly-2026-06-23", false),
            ["+nightly-2026-06-23", "fmt"]
        );
    }

    #[test]
    fn cargo_fmt_args_appends_check() {
        assert_eq!(
            cargo_fmt_args("nightly-2026-06-23", true),
            ["+nightly-2026-06-23", "fmt", "--check"]
        );
    }

    #[test]
    fn taplo_args_toggle_on_check() {
        assert_eq!(taplo_args(false), ["fmt"]);
        assert_eq!(taplo_args(true), ["fmt", "--check"]);
    }
}
