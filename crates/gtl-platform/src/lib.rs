//! Platform abstraction layer (PAL): the single home of OS-specific behavior.
//! See ADR-0003. No `#[cfg(target_os/...)]` may live outside this crate.
use std::{
    path::Path,
    process::{Command, Stdio},
};

pub mod paths;
pub mod policy;

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
    pub fn spawn_detached(_program: &Path, _args: &[&str]) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "detached viewer spawn is not implemented for this OS",
        ))
    }
    pub fn activate_window(_xid: u64) {}
}

/// Spawn `program args…` detached (fire-and-forget). Delegates to the cfg-selected
/// OS backend; best-effort daemon launch for the desktop viewer. See ADR-0003.
pub fn spawn_detached(program: &Path, args: &[&str]) -> std::io::Result<()> {
    sys::spawn_detached(program, args)
}

/// Raises and focuses the native window `xid`, even over a focused fullscreen
/// window. Best-effort and silent on failure; a no-op on OSes/sessions without
/// an EWMH-style activation primitive (see the Linux backend). The window must
/// already be mapped — callers that just un-hid it should defer slightly.
pub fn activate_window(xid: u64) {
    sys::activate_window(xid)
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

fn no_open_requested(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
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
}
