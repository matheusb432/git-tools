//! Shared process + output-contract helpers. Every verb spawns children and emits its
//! `RESULT scope=… status=…` line through this module — never spawn ad hoc per verb.
//!
//! If this repo grows a captured-output `gate` binary (runs a command, tees combined output
//! to `.artifacts/logs/<scope>.log`, tails failures, emits RESULT), add a `gate(scope, …)`
//! helper here that shells `cargo run -p gate` and delegate captured runs to it rather than
//! reimplementing capture. The template ships only the always-needed `run`/`result` stubs.
//!
//! `run_in` and `result_fail_step` are starter helpers the example verbs don't yet call;
//! the module-level `allow(dead_code)` keeps a freshly scaffolded crate warning-clean. Drop
//! the attribute once your real verbs use them (they will).
#![allow(dead_code)]

use std::process::Command;

use anyhow::{Result, bail};

/// Run `program args…`, returning an error tagged with `label` if it exits non-zero.
pub fn run(label: &str, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        bail!("{label} failed (exit {})", status.code().unwrap_or(-1));
    }
    Ok(())
}

/// Run `program args…` in `dir`, returning an error tagged with `label` on non-zero exit.
pub fn run_in(label: &str, dir: &str, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program).current_dir(dir).args(args).status()?;
    if !status.success() {
        bail!("{label} failed (exit {})", status.code().unwrap_or(-1));
    }
    Ok(())
}

/// Emit the parsed contract line on stdout (keep this byte-stable — consumers grep it).
pub fn result(scope: &str, status: &str) {
    println!("RESULT scope={scope} status={status}");
}

/// Emit a FAIL contract line naming the failed step.
pub fn result_fail_step(scope: &str, step: &str) {
    println!("RESULT scope={scope} status=FAIL step={step}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_ok_on_true() {
        assert!(run("noop", "true", &[]).is_ok());
    }

    #[test]
    fn run_err_on_false() {
        let err = run("boom", "false", &[]).unwrap_err();
        assert!(err.to_string().contains("boom failed"));
    }
}
