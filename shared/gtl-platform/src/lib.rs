//! Platform abstraction layer (PAL): the single home of OS-specific behavior.
//! See ADR-0003. No `#[cfg(target_os/...)]` may live outside this crate.
use std::{
    path::Path,
    process::{Command, Stdio},
};

pub mod paths;
pub mod policy;

mod bounded_command;

pub use bounded_command::run_command_with_bounded_stdout_in;

// OS backend selection (ADR-0003): folders named by `target_os`; one sibling per
// OS, existing arms untouched. Linux + Windows have effectful backends; other OSes
// (macOS dev) fall through to the stub and degrade to the browser path.
#[cfg(target_os = "linux")]
#[path = "linux/mod.rs"]
mod sys;

#[cfg(windows)]
#[path = "windows/mod.rs"]
mod sys;

#[cfg(not(any(target_os = "linux", windows)))]
mod sys {
    //! Stub for OSes without an effectful backend (macOS dev). The CLI degrades to
    //! the browser path when `spawn_detached` returns `Unsupported`.
    use std::path::Path;
    pub fn copy_current_executable(destination: &Path) -> std::io::Result<u64> {
        std::fs::copy(std::env::current_exe()?, destination)
    }
    pub fn spawn_detached(_program: &Path, _args: &[&str]) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "detached viewer spawn is not implemented for this OS",
        ))
    }
    pub fn spawn_detached_in(
        _program: &Path,
        _arguments: &[&str],
        _working_directory: &Path,
    ) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "detached viewer spawn is not implemented for this OS",
        ))
    }
    pub fn activate_window(_xid: u64) {}
}

/// Copies the bytes of the currently running executable to `destination`.
///
/// On Linux, this remains valid when an atomic rebuild has already unlinked the executable's
/// original path.
///
/// # Examples
///
/// ```no_run
/// # use std::path::Path;
/// gtl_platform::copy_current_executable(Path::new("runner-copy"))?;
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error when the current executable cannot be resolved or read, or when
/// `destination` cannot be created.
pub fn copy_current_executable(destination: &Path) -> std::io::Result<u64> {
    sys::copy_current_executable(destination)
}

/// Spawn `program args…` detached (fire-and-forget). Delegates to the cfg-selected
/// OS backend; best-effort daemon launch for the desktop viewer. See ADR-0003.
pub fn spawn_detached(program: &Path, args: &[&str]) -> std::io::Result<()> {
    sys::spawn_detached(program, args)
}

pub fn spawn_detached_in(
    program: &Path,
    arguments: &[&str],
    working_directory: &Path,
) -> std::io::Result<()> {
    sys::spawn_detached_in(program, arguments, working_directory)
}

/// Raises and focuses the native window `xid`, even over a focused fullscreen
/// window. Best-effort and silent on failure; a no-op on OSes/sessions without
/// an EWMH-style activation primitive (see the Linux backend). The window must
/// already be mapped — callers that just un-hid it should defer slightly.
pub fn activate_window(xid: u64) {
    sys::activate_window(xid);
}

/// The operating systems the tool targets. Windows/macOS are not yet wired for
/// every effect, but the type exists so policy decisions are total today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Macos,
    Windows,
}

impl Os {
    /// The OS this binary is running on (runtime, not `#[cfg]`).
    pub fn current() -> Os {
        match std::env::consts::OS {
            "windows" => Os::Windows,
            "macos" => Os::Macos,
            _ => Os::Linux,
        }
    }
}

/// Open `path` in the OS default browser/handler. No-op when `GIT_TOOLS_NO_OPEN`
/// is truthy. Best-effort: a spawn failure is swallowed (matches legacy `open.rs`).
pub fn open_in_browser(path: &Path) {
    if no_open_requested(std::env::var("GIT_TOOLS_NO_OPEN").ok().as_deref()) {
        return;
    }
    let is_wsl = std::env::var_os("WSL_DISTRO_NAME").is_some();
    let path_str = path.to_string_lossy();
    let command = policy::opener_command(Os::current(), is_wsl, path_str.as_ref());
    let _ = Command::new(command.program)
        .args(command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Create a directory symlink at `link` pointing to `original`, replacing any existing
/// `link` first (idempotent — mirrors `ln -sfn`). The OS `#[cfg]` for the symlink syscall
/// lives here, in the PAL, and nowhere else (ADR-0003): Unix uses `std::os::unix::fs::symlink`,
/// Windows uses `std::os::windows::fs::symlink_dir`. `original` is written through verbatim, so
/// a relative path produces a relative link.
pub fn symlink_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    // Remove a prior link (file-symlink on Unix, dir-symlink on Windows) so a re-run replaces it.
    let _ = std::fs::remove_file(link).or_else(|_| std::fs::remove_dir(link));
    symlink_dir_impl(original, link)
}

/// Creates a file symlink at `link` pointing to `original`, replacing an existing link.
pub fn symlink_file(original: &Path, link: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_file(link).or_else(|_| std::fs::remove_dir(link));
    symlink_file_impl(original, link)
}

#[cfg(unix)]
fn symlink_dir_impl(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(windows)]
fn symlink_dir_impl(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(original, link)
}

#[cfg(unix)]
fn symlink_file_impl(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(windows)]
fn symlink_file_impl(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(original, link)
}

fn no_open_requested(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_open_env_guard_accepts_truthy_values() {
        assert!(no_open_requested(Some("1")));
        assert!(no_open_requested(Some("true")));
        assert!(!no_open_requested(Some("0")));
        assert!(!no_open_requested(None));
    }

    #[test]
    fn current_executable_can_be_copied_to_a_stable_path() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let destination = temporary_directory.path().join("executable-copy");

        let copied = copy_current_executable(&destination).unwrap();

        assert!(copied > 0);
        assert!(destination.is_file());
    }

    #[test]
    fn symlink_dir_links_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("real");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("f.txt"), b"hi").unwrap();
        let link = tmp.path().join("link");

        symlink_dir(&target, &link).unwrap();
        assert_eq!(std::fs::read(link.join("f.txt")).unwrap(), b"hi");

        // `ln -sfn` semantics: re-linking an existing link replaces it, no error.
        symlink_dir(&target, &link).unwrap();
        assert_eq!(std::fs::read(link.join("f.txt")).unwrap(), b"hi");
    }

    #[test]
    fn symlink_file_links_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("real.txt");
        std::fs::write(&target, b"hi").unwrap();
        let link = tmp.path().join("link.txt");

        symlink_file(&target, &link).unwrap();
        assert_eq!(std::fs::read(&link).unwrap(), b"hi");

        symlink_file(&target, &link).unwrap();
        assert_eq!(std::fs::read(&link).unwrap(), b"hi");
    }
}
