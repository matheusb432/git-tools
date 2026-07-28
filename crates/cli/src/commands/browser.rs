use std::{
    path::Path,
    process::{Command, Stdio},
};

pub(super) fn open(path: &Path) {
    let is_wsl = std::env::var_os("WSL_DISTRO_NAME").is_some();
    let path = path.to_string_lossy();
    let _ = Command::new(opener_program(std::env::consts::OS, is_wsl))
        .arg(path.as_ref())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn opener_program(operating_system: &str, is_wsl: bool) -> &'static str {
    if is_wsl || operating_system == "windows" {
        "explorer.exe"
    } else if operating_system == "macos" {
        "open"
    } else {
        "xdg-open"
    }
}

#[cfg(test)]
mod tests {
    use super::opener_program;

    #[test]
    fn opener_program_matches_each_supported_environment() {
        assert_eq!(opener_program("linux", true), "explorer.exe");
        assert_eq!(opener_program("windows", false), "explorer.exe");
        assert_eq!(opener_program("macos", false), "open");
        assert_eq!(opener_program("linux", false), "xdg-open");
    }
}
