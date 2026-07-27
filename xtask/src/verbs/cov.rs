//! Rust workspace coverage orchestration.

use anyhow::Result;

use crate::process;

const COMMAND: &str = "cargo llvm-cov --workspace --no-cfg-coverage --text --output-dir .artifacts/coverage --show-missing-lines --ignore-filename-regex '(/tests/|/tests\\.rs$|_tests\\.rs$)'";

/// Run every Rust workspace test and write a production-line coverage report.
pub(crate) fn run() -> Result<()> {
    process::gate("cov", COMMAND, false)
}
