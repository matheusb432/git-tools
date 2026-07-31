//! `xtask bootstrap` is the repository-local phase of development-host bring-up. Mise installs
//! Ubuntu packages and toolchains first, then invokes this verb as its bootstrap task.
//!
//! Steps (CWD is the repository root):
//! 1. configure the tracked `.githooks` directory for this clone;
//! 2. install the pinned Deno dependencies;
//! 3. build both artifacts (`just build`);
//! 4. install both onto PATH (`install::run_install`);
//! 5. ensure `~/.local/bin` is on PATH (append to `~/.zshrc` once, when absent).

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
        bail!("`just` not found on PATH; run `mise install just`, then re-run bootstrap");
    }
    which::which("deno").context("required tool `deno` is missing; run `mise install deno`")?;
    configure_git_hooks()?;
    process::run("frontend-dependencies", "deno", &["install", "--frozen"])?;
    process::run("build", "just", &["build"])?;
    install::run_install(InstallTarget::Both)?;
    ensure_path_on_zshrc()?;
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

/// Append a PATH export to `~/.zshrc` when the install bindir is not already on PATH. Idempotent
/// in practice: once a reopened shell has the dir on PATH, this is a no-op.
fn ensure_path_on_zshrc() -> Result<()> {
    let bindir = install::bindir()?;
    let path_var = env::var("PATH").unwrap_or_default();
    if !needs_path_entry(&path_var, &bindir) {
        return Ok(());
    }
    let home = env::var_os("HOME").context("HOME is not set; cannot update ~/.zshrc for PATH")?;
    let zshrc = PathBuf::from(home).join(".zshrc");
    let line = format!("export PATH=\"{}:$PATH\"\n", bindir.display());
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&zshrc)
        .with_context(|| format!("appending the PATH export to {}", zshrc.display()))?;
    f.write_all(line.as_bytes())?;
    println!(
        "Added {} to PATH in ~/.zshrc (open a new shell).",
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
