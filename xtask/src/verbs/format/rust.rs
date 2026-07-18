//! Rust formatting plan (pinned-nightly rustfmt).
//!
//! The nightly toolchain is read from `.rustfmt-nightly`: stable `cargo fmt` silently skips this
//! repo's nightly-only `rustfmt.toml` keys, so the `+toolchain` token is mandatory (see
//! rust-style).

use std::{env, fs};

use anyhow::{Context, Result};

use super::FormatMode;
use crate::task::Step;

const NIGHTLY_FILE: &str = ".rustfmt-nightly";

/// `cargo +<nightly> fmt [--check]`, with the pinned toolchain read from `.rustfmt-nightly`.
pub(super) fn format_step(mode: FormatMode) -> Result<Step> {
    let toolchain = read_nightly()?;
    Ok(Step::new(
        "cargo-fmt",
        "cargo",
        fmt_arguments(&toolchain, mode),
    ))
}

/// The `cargo` argv. The `+toolchain` token must come first so the rustup proxy selects the pinned
/// nightly before `fmt` runs.
fn fmt_arguments(toolchain: &str, mode: FormatMode) -> Vec<String> {
    let mut arguments = vec![format!("+{toolchain}"), "fmt".to_string()];
    if let Some(flag) = mode.check_argument() {
        arguments.push(flag.to_string());
    }
    arguments
}

/// Read the pinned rustfmt nightly toolchain name from `.rustfmt-nightly` (repo root = CWD).
fn read_nightly() -> Result<String> {
    let path = env::current_dir()?.join(NIGHTLY_FILE);
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("reading {} for the pinned rustfmt nightly", path.display()))?;
    Ok(raw.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_by_default() {
        assert_eq!(
            fmt_arguments("nightly-2026-06-23", FormatMode::Write),
            ["+nightly-2026-06-23", "fmt"]
        );
    }

    #[test]
    fn appends_check() {
        assert_eq!(
            fmt_arguments("nightly-2026-06-23", FormatMode::Check),
            ["+nightly-2026-06-23", "fmt", "--check"]
        );
    }
}
