//! Frontend build and test orchestration. Deno owns dependency resolution; package.json owns
//! the concrete TypeScript, Oxc, Vitest, and Vite commands.

use std::{ffi::OsStr, path::Path};

use anyhow::Result;

use crate::{process, project};

/// Build the committed offline viewer bundle in production mode.
pub fn build() -> Result<()> {
    let root = project::repository_root();
    let _lock = project::lock_frontend_assets(&root)?;
    build_unlocked(&root)
}

pub(crate) fn build_unlocked(root: &Path) -> Result<()> {
    process::run_captured_with_env(
        "frontend-build",
        Some(root),
        OsStr::new("deno"),
        &["task", "--frozen", "build"],
        &[("NODE_ENV", OsStr::new("production"))],
    )
}

/// Type-check and run the framework-free frontend unit tests.
pub fn test() -> Result<()> {
    process::run(
        "frontend-typecheck",
        "deno",
        &["task", "--frozen", "typecheck"],
    )?;
    process::run("frontend-test", "deno", &["task", "--frozen", "test"])
}

/// Run the frontend compute benchmarks and stream their tables.
pub fn bench() -> Result<()> {
    process::run("frontend-bench", "deno", &["task", "--frozen", "bench"])
}
