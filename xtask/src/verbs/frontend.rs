//! Frontend build and test orchestration. Deno owns dependency resolution; package.json owns
//! the concrete TypeScript, Oxc, Vitest, and Vite commands.

use std::ffi::OsStr;

use anyhow::{Context, Result};

use crate::process;

fn require_deno() -> Result<()> {
    which::which("deno").context(
        "required tool `deno` is missing; install it through the declarative host configuration",
    )?;
    Ok(())
}

/// Build the committed offline viewer bundle in production mode.
pub fn build() -> Result<()> {
    require_deno()?;
    process::run_captured_with_env(
        "frontend-build",
        None,
        OsStr::new("deno"),
        &["task", "--frozen", "build"],
        &[("NODE_ENV", OsStr::new("production"))],
    )
}

/// Type-check and run the framework-free frontend unit tests.
pub fn test() -> Result<()> {
    require_deno()?;
    process::run(
        "frontend-typecheck",
        "deno",
        &["task", "--frozen", "typecheck"],
    )?;
    process::run("frontend-test", "deno", &["task", "--frozen", "test"])
}
