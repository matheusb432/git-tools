//! `xtask bootstrap` — the full post-toolchain dev-host bring-up. `bootstrap.sh` installs *only*
//! the Rust toolchain (the chicken-and-egg seam), then `exec`s this verb, which does every other
//! automation. Migrates `install-git-tools.sh` (build + install + ensure PATH) into one Rust
//! verb.
//!
//! Steps (CWD is the repo root — `bootstrap.sh` cds there, and `just bootstrap` runs from root):
//! 1. configure the tracked `.githooks` directory for this clone;
//! 2. install the pinned Deno dependencies;
//! 3. build both artifacts (`just build`);
//! 4. install both onto PATH (`install::run_install`);
//! 5. ensure `~/.local/bin` is on PATH (append to `~/.bashrc` once, when absent).

use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use super::install;
use crate::{
    cli::InstallTarget,
    process::{self, Status},
    verb::Verb,
};

/// Run the bring-up. `just` is required (it drives the build); fail loud with a fix hint when
/// absent rather than a cryptic mid-build error.
pub fn run() -> Result<()> {
    if which::which("just").is_err() {
        bail!("`just` not found on PATH — install it (e.g. `cargo install just`) then re-run");
    }
    which::which("deno").context(
        "required tool `deno` is missing; install it through the declarative host configuration",
    )?;
    configure_git_hooks()?;
    process::run("frontend-dependencies", "deno", &["install", "--frozen"])?;
    process::run("build", "just", &["build"])?;
    install::run_install(InstallTarget::Both)?;
    ensure_path_on_bashrc()?;
    process::result(Verb::BOOTSTRAP, Status::Pass);
    Ok(())
}

fn configure_git_hooks() -> Result<()> {
    process::run(
        "git-hooks",
        "git",
        &["config", "core.hooksPath", ".githooks"],
    )
}

/// Whether `bindir` is absent from the colon-separated `path_var` (so it must be added to the rc).
fn needs_path_entry(path_var: &str, bindir: &Path) -> bool {
    !path_var
        .split(':')
        .filter(|p| !p.is_empty())
        .any(|p| Path::new(p) == bindir)
}

/// Append a PATH export to `~/.bashrc` when the install bindir is not already on PATH. Idempotent
/// in practice: once a reopened shell has the dir on PATH, this is a no-op.
fn ensure_path_on_bashrc() -> Result<()> {
    let bindir = install::bindir()?;
    let path_var = env::var("PATH").unwrap_or_default();
    if !needs_path_entry(&path_var, &bindir) {
        return Ok(());
    }
    let home = env::var_os("HOME").context("HOME is not set; cannot update ~/.bashrc for PATH")?;
    let bashrc = PathBuf::from(home).join(".bashrc");
    let line = format!("export PATH=\"{}:$PATH\"\n", bindir.display());
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&bashrc)
        .with_context(|| format!("appending the PATH export to {}", bashrc.display()))?;
    f.write_all(line.as_bytes())?;
    println!(
        "Added {} to PATH in ~/.bashrc (open a new shell).",
        bindir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_path_entry_detects_presence_and_absence() {
        let bindir = Path::new("/home/x/.local/bin");
        assert!(!needs_path_entry("/usr/bin:/home/x/.local/bin", bindir));
        assert!(needs_path_entry("/usr/bin:/bin", bindir));
        assert!(needs_path_entry("", bindir));
    }
}
