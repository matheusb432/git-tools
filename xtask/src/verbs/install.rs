//! `xtask install` / `xtask uninstall` — place or remove the prebuilt git-tools CLI (+ the
//! `gtl` alias + the resident `gtl-daemon`) and the gtl-viewer desktop binary on PATH. Migrates
//! `scripts/install.sh`: the build is owned by the justfile (`just cli build` / `just desktop
//! build`); these verbs only copy the already-built artifacts. Idempotent; the destructive
//! config removal refuses non-interactively unless `--force`. `gtl-daemon` may be running
//! during `just update` — placed via the same atomic replace as the viewer, so the swap never
//! fails with "Text file busy"; the client's exe-identity handshake restarts it on the next
//! CLI invocation.
//!
//! Pure placement helpers (byte-compare, atomic replace, the installed|updated|unchanged
//! contract) are unit-tested with `tempfile`; the thin glue that resolves repo root / bindir
//! and prints the report is covered by the arg-surface tests.

use std::{
    env, fmt, fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::cli::InstallTarget;

mod linux_desktop;

// --- placement contract ----------------------------------------------------

/// What placing one file did: was it new, changed, or already current? The lowercased
/// `Display` is the `installed|updated|unchanged` string the `ShellSpec` suite asserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Installed,
    Updated,
    Unchanged,
}

impl Action {
    /// Combine two placements into the louder one: updated > installed > unchanged. Mirrors the
    /// shell's combined reporting when a binary and its alias land in one call.
    fn louder(self, other: Action) -> Action {
        use Action::{Installed, Unchanged, Updated};
        match (self, other) {
            (Updated, _) | (_, Updated) => Updated,
            (Installed, _) | (_, Installed) => Installed,
            _ => Unchanged,
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Action::Installed => "installed",
            Action::Updated => "updated",
            Action::Unchanged => "unchanged",
        })
    }
}

/// Outcome of an uninstall sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    Removed,
    Nothing,
}

/// Outcome of the guarded config delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigRemoval {
    Removed,
    Kept,
    Absent,
}

// --- pure placement helpers (unit-tested) ----------------------------------

/// The host's executable suffix: `.exe` under Windows, empty elsewhere — no `uname` branch.
fn exe_suffix() -> &'static str {
    env::consts::EXE_SUFFIX
}

fn cli_bin_name() -> String {
    format!("git-tools{}", exe_suffix())
}

fn cli_alias_name() -> String {
    format!("gtl{}", exe_suffix())
}

fn viewer_bin_name() -> String {
    format!("gtl-viewer{}", exe_suffix())
}

fn daemon_bin_name() -> String {
    format!("gtl-daemon{}", exe_suffix())
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
}

/// Replace `dst` with `src` atomically: stage a copy in `dst`'s directory, then rename over the
/// target. `rename` onto a busy executable swaps the inode instead of writing in place, so an
/// in-place update of a running binary (the keep-warm gtl-viewer tray app) never fails with
/// "Text file busy". `fs::copy` carries the source's mode bits on Unix, so the staged exe stays
/// executable without an OS-specific chmod.
fn atomic_replace(src: &Path, dst: &Path) -> io::Result<()> {
    let dir = dst.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.xtask-tmp",
        file_name(dst),
        std::process::id()
    ));
    fs::copy(src, &tmp)?;
    if let Err(e) = fs::rename(&tmp, dst) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Copy `src` over `dst` only when the bytes differ. Echoes the action taken.
fn copy_if_changed(src: &Path, dst: &Path) -> io::Result<Action> {
    if dst.exists() {
        if fs::read(src)? == fs::read(dst)? {
            return Ok(Action::Unchanged);
        }
        atomic_replace(src, dst)?;
        return Ok(Action::Updated);
    }
    atomic_replace(src, dst)?;
    Ok(Action::Installed)
}

/// Install CLI exe `src` plus the `gtl` alias into `bindir`. Echoes the combined action
/// (updated > installed > unchanged), mirroring the retired `install_cli_binary`.
pub fn install_cli_binary(src: &Path, bindir: &Path) -> io::Result<Action> {
    fs::create_dir_all(bindir)?;
    let primary = copy_if_changed(src, &bindir.join(file_name(src)))?;
    let alias = copy_if_changed(src, &bindir.join(cli_alias_name()))?;
    Ok(primary.louder(alias))
}

/// Install exe `src` into `bindir` via atomic replace, so a warm running process (the tray
/// viewer or the resident daemon) can be updated in place without "Text file busy". Shared by
/// `install_viewer_binary` and `install_daemon_binary`, which differ only in which binary they
/// place. Echoes the action.
fn install_binary_atomic(src: &Path, bindir: &Path) -> io::Result<Action> {
    fs::create_dir_all(bindir)?;
    copy_if_changed(src, &bindir.join(file_name(src)))
}

/// Install the desktop viewer exe `src` into `bindir` via atomic replace, so a warm tray viewer
/// can be updated in place. Echoes the action.
pub fn install_viewer_binary(src: &Path, bindir: &Path) -> io::Result<Action> {
    install_binary_atomic(src, bindir)
}

/// Install the `gtl-daemon` exe `src` into `bindir` via atomic replace, so a resident daemon can
/// be updated while running — the client's exe-identity handshake restarts it on the next CLI
/// invocation. Echoes the action.
pub fn install_daemon_binary(src: &Path, bindir: &Path) -> io::Result<Action> {
    install_binary_atomic(src, bindir)
}

/// Remove the CLI binary + `gtl` alias from `bindir`.
pub fn uninstall_cli_binary(bindir: &Path) -> io::Result<Removal> {
    let mut removed = false;
    for name in [cli_bin_name(), cli_alias_name()] {
        let path = bindir.join(name);
        if path.exists() {
            fs::remove_file(&path)?;
            removed = true;
        }
    }
    Ok(if removed {
        Removal::Removed
    } else {
        Removal::Nothing
    })
}

/// Delete config file `path`, guarded. Proceeds when `force`, or when `confirm()` returns true;
/// otherwise keeps the file. The interactivity is injected as a closure so the decision is
/// host-testable without a real TTY.
pub fn remove_cli_config(
    path: &Path,
    force: bool,
    confirm: &dyn Fn() -> bool,
) -> io::Result<ConfigRemoval> {
    if !path.exists() {
        return Ok(ConfigRemoval::Absent);
    }
    if !force && !confirm() {
        return Ok(ConfigRemoval::Kept);
    }
    fs::remove_file(path)?;
    Ok(ConfigRemoval::Removed)
}

// --- thin glue (resolve paths, print the report) ---------------------------

/// Target bindir: `GIT_TOOLS_BINDIR` override, else `$HOME/.local/bin`. Shared with `bootstrap`
/// (which ensures this dir is on PATH).
pub(crate) fn bindir() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("GIT_TOOLS_BINDIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = env::var_os("HOME").context("HOME is not set; cannot resolve the install bindir")?;
    Ok(PathBuf::from(home).join(".local").join("bin"))
}

/// Place the requested artifact(s). Repo root is the current dir (the install recipes set
/// `working-directory := '..'`, so CWD is the repo root).
pub fn run_install(target: InstallTarget) -> Result<()> {
    let repo_path = env::current_dir()?;
    let bindir = bindir()?;
    match target {
        InstallTarget::Cli => install_cli(&repo_path, &bindir),
        InstallTarget::Viewer => install_viewer(&repo_path, &bindir),
        InstallTarget::Both => {
            install_cli(&repo_path, &bindir)?;
            install_viewer(&repo_path, &bindir)
        }
    }
}

fn install_cli(repo_path: &Path, bindir: &Path) -> Result<()> {
    let src = repo_path
        .join("target")
        .join("release")
        .join(cli_bin_name());
    if !src.is_file() {
        bail!(
            "git-tools not built at {} — run `just cli build` first",
            src.display()
        );
    }
    let act = install_cli_binary(&src, bindir)?;
    println!(
        "git-tools {act} -> {}",
        bindir.join(cli_bin_name()).display()
    );
    println!("gtl {act} -> {}", bindir.join(cli_alias_name()).display());

    let daemon_src = repo_path
        .join("target")
        .join("release")
        .join(daemon_bin_name());
    if !daemon_src.is_file() {
        bail!(
            "gtl-daemon not built at {} — run `just cli build` first",
            daemon_src.display()
        );
    }
    let daemon_act = install_daemon_binary(&daemon_src, bindir)?;
    println!(
        "gtl-daemon {daemon_act} -> {}",
        bindir.join(daemon_bin_name()).display()
    );
    Ok(())
}

fn install_viewer(repo_path: &Path, bindir: &Path) -> Result<()> {
    let src = repo_path
        .join("target")
        .join("release")
        .join(viewer_bin_name());
    if src.is_file() {
        let dst = bindir.join(viewer_bin_name());
        let act = install_viewer_binary(&src, bindir)?;
        println!("gtl-viewer {act} -> {}", dst.display());
        if let Some(path) = linux_desktop::install(repo_path, &dst)? {
            println!("gtl-viewer desktop entry -> {}", path.display());
        }
    } else {
        eprintln!("gtl-viewer not built (viewer is optional; browser fallback active)");
    }
    Ok(())
}

/// Remove the installed binaries; optionally delete the repo-local config files.
pub fn run_uninstall(remove_config: bool, force: bool) -> Result<()> {
    let bindir = bindir()?;
    match uninstall_cli_binary(&bindir)? {
        Removal::Removed => println!("removed {}", bindir.join(cli_bin_name()).display()),
        Removal::Nothing => println!(
            "nothing to remove at {}",
            bindir.join(cli_bin_name()).display()
        ),
    }
    let daemon = bindir.join(daemon_bin_name());
    if daemon.exists() {
        fs::remove_file(&daemon)?;
        println!("removed {}", daemon.display());
    }
    let viewer = bindir.join(viewer_bin_name());
    if viewer.exists() {
        fs::remove_file(&viewer)?;
        println!("removed {}", viewer.display());
    }
    for path in linux_desktop::uninstall()? {
        println!("removed {}", path.display());
    }
    if remove_config {
        let cwd = env::current_dir()?;
        for name in ["git-tools.toml", "git-tools.secrets.toml"] {
            let path = cwd.join(name);
            remove_cli_config(&path, force, &|| confirm_config_delete(&path))?;
        }
    }
    Ok(())
}

/// Real interactive guard: refuse non-interactively (with a fix hint), else prompt y/N.
fn confirm_config_delete(path: &Path) -> bool {
    use io::IsTerminal;
    if !io::stdin().is_terminal() {
        eprintln!(
            "Delete config file '{}'? [refused: non-interactive, pass --force to proceed]",
            path.display()
        );
        return false;
    }
    eprint!("Delete config file '{}'? [y/N] ", path.display());
    let mut ans = String::new();
    if io::stdin().read_line(&mut ans).is_err() {
        return false;
    }
    matches!(ans.trim(), "y" | "Y" | "yes" | "Yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp bindir + a `git-tools` source file holding `bytes`. Both `TempDir` guards are
    /// returned so they auto-clean on drop; `src` is the source path, `bindir` the target dir.
    fn fixture(bytes: &[u8]) -> (tempfile::TempDir, tempfile::TempDir, PathBuf, PathBuf) {
        fixture_named("git-tools", bytes)
    }

    /// Like `fixture`, but the source file is named `name` — lets a test prove the *right*
    /// file name lands in `bindir` (e.g. the daemon binary), not just that bytes copy correctly.
    fn fixture_named(
        name: &str,
        bytes: &[u8],
    ) -> (tempfile::TempDir, tempfile::TempDir, PathBuf, PathBuf) {
        let bindir = tempfile::tempdir().unwrap();
        let srcdir = tempfile::tempdir().unwrap();
        let src = srcdir.path().join(name);
        fs::write(&src, bytes).unwrap();
        let bindir_path = bindir.path().to_path_buf();
        (bindir, srcdir, src, bindir_path)
    }

    #[test]
    fn install_cli_binary_places_binary_and_alias() {
        let (_bin, _srcdir, src, bindir) = fixture(b"v1");
        assert_eq!(
            install_cli_binary(&src, &bindir).unwrap(),
            Action::Installed
        );
        assert!(bindir.join("git-tools").exists());
        assert!(bindir.join("gtl").exists());
        assert_eq!(fs::read(bindir.join("gtl")).unwrap(), b"v1");
    }

    #[test]
    fn install_cli_binary_is_idempotent_on_identical_bytes() {
        let (_bin, _srcdir, src, bindir) = fixture(b"v1");
        install_cli_binary(&src, &bindir).unwrap();
        assert_eq!(
            install_cli_binary(&src, &bindir).unwrap(),
            Action::Unchanged
        );
    }

    #[test]
    fn install_cli_binary_updates_when_bytes_change() {
        let (_bin, _srcdir, src, bindir) = fixture(b"v1");
        install_cli_binary(&src, &bindir).unwrap();
        fs::write(&src, b"v2").unwrap();
        assert_eq!(install_cli_binary(&src, &bindir).unwrap(), Action::Updated);
    }

    #[test]
    fn install_viewer_binary_places_and_updates() {
        let (_bin, _srcdir, src, bindir) = fixture(b"v1");
        assert_eq!(
            install_viewer_binary(&src, &bindir).unwrap(),
            Action::Installed
        );
        assert!(bindir.join("git-tools").exists());
        fs::write(&src, b"v2").unwrap();
        assert_eq!(
            install_viewer_binary(&src, &bindir).unwrap(),
            Action::Updated
        );
        assert_eq!(fs::read(bindir.join("git-tools")).unwrap(), b"v2");
    }

    #[test]
    fn install_daemon_binary_places_the_daemon_exe_name() {
        let (_bin, _srcdir, src, bindir) = fixture_named(&daemon_bin_name(), b"v1");
        assert_eq!(
            install_daemon_binary(&src, &bindir).unwrap(),
            Action::Installed
        );
        assert!(bindir.join(daemon_bin_name()).exists());
        assert!(!bindir.join("git-tools").exists());
        fs::write(&src, b"v2").unwrap();
        assert_eq!(
            install_daemon_binary(&src, &bindir).unwrap(),
            Action::Updated
        );
        assert_eq!(fs::read(bindir.join(daemon_bin_name())).unwrap(), b"v2");
    }

    #[test]
    fn atomic_replace_leaves_no_temp_files() {
        let (_bin, _srcdir, src, bindir) = fixture(b"v1");
        install_viewer_binary(&src, &bindir).unwrap();
        fs::write(&src, b"v2").unwrap();
        install_viewer_binary(&src, &bindir).unwrap();
        let leftovers: Vec<_> = fs::read_dir(&bindir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".git-tools"))
            .collect();
        assert!(leftovers.is_empty(), "leftover temp files: {leftovers:?}");
    }

    #[test]
    fn uninstall_removes_binary_and_alias() {
        let (_bin, _srcdir, src, bindir) = fixture(b"v1");
        install_cli_binary(&src, &bindir).unwrap();
        assert_eq!(uninstall_cli_binary(&bindir).unwrap(), Removal::Removed);
        assert!(!bindir.join("git-tools").exists());
        assert!(!bindir.join("gtl").exists());
    }

    #[test]
    fn uninstall_reports_nothing_when_absent() {
        let (_bin, _srcdir, _src, bindir) = fixture(b"v1");
        assert_eq!(uninstall_cli_binary(&bindir).unwrap(), Removal::Nothing);
    }

    #[test]
    fn remove_config_kept_when_confirm_declines() {
        let (_bin, _srcdir, _src, bindir) = fixture(b"v1");
        let cfg = bindir.join("git-tools.toml");
        fs::write(&cfg, b"default=\"x\"").unwrap();
        assert_eq!(
            remove_cli_config(&cfg, false, &|| false).unwrap(),
            ConfigRemoval::Kept
        );
        assert!(cfg.exists());
    }

    #[test]
    fn remove_config_proceeds_with_force() {
        let (_bin, _srcdir, _src, bindir) = fixture(b"v1");
        let cfg = bindir.join("git-tools.toml");
        fs::write(&cfg, b"default=\"x\"").unwrap();
        assert_eq!(
            remove_cli_config(&cfg, true, &|| false).unwrap(),
            ConfigRemoval::Removed
        );
        assert!(!cfg.exists());
    }

    #[test]
    fn remove_config_absent_is_reported() {
        let (_bin, _srcdir, _src, bindir) = fixture(b"v1");
        let cfg = bindir.join("does-not-exist.toml");
        assert_eq!(
            remove_cli_config(&cfg, true, &|| true).unwrap(),
            ConfigRemoval::Absent
        );
    }
}
