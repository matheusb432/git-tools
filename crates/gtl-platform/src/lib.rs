//! Platform abstraction layer (PAL): the single home of OS-specific behavior.
//! See ADR-0003. No `#[cfg(target_os/...)]` may live outside this crate.
use std::path::Path;
use std::process::{Command, Stdio};

pub mod paths;
pub mod policy;

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
