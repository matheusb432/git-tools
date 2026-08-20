//! Platform-specific process detachment for the graphical viewer.

use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;

#[cfg(windows)]
mod windows;

/// Starts the viewer independently from the short-lived CLI process.
pub(crate) fn spawn(program: &Path, arguments: &[&str]) -> std::io::Result<()> {
    platform_spawn(program, arguments)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn platform_spawn(program: &Path, arguments: &[&str]) -> std::io::Result<()> {
    unix::spawn(program, arguments)
}

#[cfg(windows)]
fn platform_spawn(program: &Path, arguments: &[&str]) -> std::io::Result<()> {
    windows::spawn(program, arguments)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_spawn(_program: &Path, _arguments: &[&str]) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "detached viewer spawning is not implemented for this OS",
    ))
}
