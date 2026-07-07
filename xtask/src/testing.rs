//! `xtask test [--verbose] [--all]` — run the test suite. Migrates `just test` (the `ShellSpec`
//! install suite it used to also run is retired; its coverage is now `cargo test` Rust tests).
//! Terse by default (`cargo test --quiet`); `--verbose` streams full output; `--all` additionally
//! runs the bun frontend unit tests (`just cli test-js`) — the bun build/test stays bun, this only
//! orchestrates it.

use anyhow::Result;

use crate::proc;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_test_args_are_quiet_by_default() {
        assert_eq!(cargo_test_args(false), ["test", "--quiet"]);
    }

    #[test]
    fn cargo_test_args_stream_under_verbose() {
        assert_eq!(cargo_test_args(true), ["test", "--", "--nocapture"]);
    }
}
