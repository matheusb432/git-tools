//! Repository-local dependency, hook, build, and installation setup.

use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use super::install;
use crate::{
    cli::InstallTarget,
    process::{self, Status},
    verb::Verb,
};

pub(crate) fn run() -> Result<()> {
    configure_git_hooks()?;
    process::run("build", "just", &["build"])?;
    install::run_install(InstallTarget::Both)?;
    ensure_path_on_zshrc()?;
    process::result(Verb::SETUP, Status::Pass);
    Ok(())
}

fn configure_git_hooks() -> Result<()> {
    process::run(
        "git-hooks",
        "git",
        &["config", "core.hooksPath", ".githooks"],
    )
}

fn needs_path_entry(path_var: &str, binary_directory: &Path) -> bool {
    !path_var
        .split(':')
        .filter(|path| !path.is_empty())
        .any(|path| Path::new(path) == binary_directory)
}

fn ensure_path_on_zshrc() -> Result<()> {
    let binary_directory = install::bindir()?;
    let path_var = env::var("PATH").unwrap_or_default();
    if !needs_path_entry(&path_var, &binary_directory) {
        return Ok(());
    }
    let home = env::var_os("HOME").context("HOME is not set; cannot update ~/.zshrc for PATH")?;
    let zshrc = PathBuf::from(home).join(".zshrc");
    let line = format!("export PATH=\"{}:$PATH\"\n", binary_directory.display());
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&zshrc)
        .with_context(|| format!("appending the PATH export to {}", zshrc.display()))?;
    file.write_all(line.as_bytes())?;
    println!(
        "Added {} to PATH in ~/.zshrc (open a new shell).",
        binary_directory.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_path_entry_detects_presence_and_absence() {
        let binary_directory = Path::new("/home/x/.local/bin");
        assert!(!needs_path_entry(
            "/usr/bin:/home/x/.local/bin",
            binary_directory
        ));
        assert!(needs_path_entry("/usr/bin:/bin", binary_directory));
        assert!(needs_path_entry("", binary_directory));
    }
}
