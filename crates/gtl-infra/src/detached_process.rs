use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;

#[cfg(windows)]
mod windows;

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
        "detached process spawning is not implemented for this OS",
    ))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn platform_spawn_in(
    program: &Path,
    arguments: &[&str],
    working_directory: &Path,
) -> std::io::Result<()> {
    unix::spawn_in(program, arguments, working_directory)
}

#[cfg(windows)]
fn platform_spawn_in(
    program: &Path,
    arguments: &[&str],
    working_directory: &Path,
) -> std::io::Result<()> {
    windows::spawn_in(program, arguments, working_directory)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_spawn_in(
    _program: &Path,
    _arguments: &[&str],
    _working_directory: &Path,
) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "detached process spawning is not implemented for this OS",
    ))
}

/// Spawns a process detached from the current process.
///
/// # Examples
///
/// ```no_run
/// gtl_infra::detached_process::spawn(std::path::Path::new("gtl-viewer"), &[])?;
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error when the process cannot be detached and started, or when the
/// target operating system has no detached-process implementation.
pub fn spawn(program: &Path, arguments: &[&str]) -> std::io::Result<()> {
    platform_spawn(program, arguments)
}

/// Spawns a process detached from the current process in a working directory.
///
/// # Examples
///
/// ```no_run
/// gtl_infra::detached_process::spawn_in(
///     std::path::Path::new("code"),
///     &["--wait"],
///     std::path::Path::new("."),
/// )?;
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error when the process cannot be detached and started, or when the
/// target operating system has no detached-process implementation.
pub fn spawn_in(
    program: &Path,
    arguments: &[&str],
    working_directory: &Path,
) -> std::io::Result<()> {
    platform_spawn_in(program, arguments, working_directory)
}
