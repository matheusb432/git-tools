//! Resolves and starts the single-instance desktop shell from the server process.

use std::path::PathBuf;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;
#[cfg(windows)]
mod windows;

#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenViewerError {
    #[error("gtl-viewer is not installed")]
    NotInstalled,
    #[error("failed to start gtl-viewer")]
    Start(#[source] std::io::Error),
}

/// Starts the viewer or asks its existing single-instance process to focus its window.
pub(crate) fn open() -> Result<(), OpenViewerError> {
    #[cfg(feature = "benchmark-support")]
    if std::env::var_os("GTL_BENCHMARK_DISABLE_VIEWER_LAUNCH").is_some() {
        return Err(OpenViewerError::NotInstalled);
    }
    let executable = resolve_viewer_bin().ok_or(OpenViewerError::NotInstalled)?;
    platform_spawn(&executable).map_err(OpenViewerError::Start)
}

fn resolve_viewer_bin() -> Option<PathBuf> {
    let name = format!("gtl-viewer{}", std::env::consts::EXE_SUFFIX);
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        let sibling = directory.join(&name);
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(&name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn platform_spawn(program: &std::path::Path) -> std::io::Result<()> {
    unix::spawn(program)
}

#[cfg(windows)]
fn platform_spawn(program: &std::path::Path) -> std::io::Result<()> {
    windows::spawn(program)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_spawn(_program: &std::path::Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "desktop viewer startup is not implemented for this OS",
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn viewer_name_uses_the_target_executable_suffix() {
        assert_eq!(
            format!("gtl-viewer{}", std::env::consts::EXE_SUFFIX),
            if cfg!(windows) {
                "gtl-viewer.exe"
            } else {
                "gtl-viewer"
            }
        );
    }
}
