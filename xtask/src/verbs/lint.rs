//! Lint verb — the read-only linter sweep and the autofix plan.
//!
//! `run` is the standalone `lint` verb; `linters` is the shared read-only sweep the aggregate
//! `check` gate reuses, and `fix` applies the machine-applicable subset.

use anyhow::Result;

use crate::{
    process::{self, Status},
    verb::Verb,
};

/// Run every linter (read-only), then emit the `RESULT` line.
pub(crate) fn run() -> Result<()> {
    linters()?;
    process::result(Verb::LINT, Status::Pass);
    Ok(())
}

/// The read-only linter sweep: architecture lints, dependency checks, and full-workspace Clippy.
/// Shared with the aggregate `check` gate.
pub(super) fn linters() -> Result<()> {
    super::check_structure::run(None);
    process::run(
        "gtl-parser-wasm-core",
        "cargo",
        &[
            "check",
            "--locked",
            "-p",
            "gtl-parser",
            "--no-default-features",
            "--target",
            "wasm32-unknown-unknown",
        ],
    )?;
    process::run(
        "gtl-parser-wasm-all-features",
        "cargo",
        &[
            "check",
            "--locked",
            "-p",
            "gtl-parser",
            "--all-features",
            "--all-targets",
            "--target",
            "wasm32-unknown-unknown",
        ],
    )?;
    process::run(
        "clippy",
        "cargo",
        &["clippy", "--workspace", "--all-targets"],
    )
}

/// Machine-applicable Clippy fixes plus any `clippy_extra`.
pub(super) fn fix(clippy_extra: &[String]) -> Result<()> {
    let mut args = vec![
        "clippy".to_string(),
        "--workspace".to_string(),
        "--all-targets".to_string(),
        "--fix".to_string(),
        "--allow-dirty".to_string(),
        "--allow-staged".to_string(),
    ];
    args.extend_from_slice(clippy_extra);
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    process::run("clippy-fix", "cargo", &args)
}
