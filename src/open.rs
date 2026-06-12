use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenerCommand {
    program: String,
    args: Vec<String>,
}

pub fn open_file(path: impl AsRef<Path>) {
    if no_open_requested(std::env::var("GIT_TOOLS_NO_OPEN").ok().as_deref()) {
        return;
    }

    let path = path.as_ref().to_string_lossy().to_string();
    let command = opener_command(
        std::env::consts::OS,
        std::env::var_os("WSL_DISTRO_NAME").is_some(),
        &path,
    );

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

fn opener_command(platform: &str, is_wsl: bool, path: &str) -> OpenerCommand {
    if is_wsl || platform == "windows" {
        return OpenerCommand {
            program: "explorer.exe".to_string(),
            args: vec![path.to_string()],
        };
    }

    match platform {
        "macos" => OpenerCommand {
            program: "open".to_string(),
            args: vec![path.to_string()],
        },
        _ => OpenerCommand {
            program: "xdg-open".to_string(),
            args: vec![path.to_string()],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wsl_uses_windows_explorer_before_linux_platform_fallback() {
        let command = opener_command("linux", true, "out.html");

        assert_eq!(command.program, "explorer.exe");
        assert_eq!(command.args, vec!["out.html"]);
    }

    #[test]
    fn macos_uses_open() {
        let command = opener_command("macos", false, "out.html");

        assert_eq!(command.program, "open");
        assert_eq!(command.args, vec!["out.html"]);
    }

    #[test]
    fn windows_uses_explorer_without_shell_parsing() {
        let command = opener_command("windows", false, "out.html");

        assert_eq!(command.program, "explorer.exe");
        assert_eq!(command.args, vec!["out.html"]);
    }

    #[test]
    fn windows_passes_metacharacter_paths_as_one_argument() {
        let command = opener_command("windows", false, r"C:\tmp\a&b.html");

        assert_eq!(command.program, "explorer.exe");
        assert_eq!(command.args, vec![r"C:\tmp\a&b.html"]);
    }

    #[test]
    fn no_open_env_guard_accepts_truthy_values() {
        assert!(no_open_requested(Some("1")));
        assert!(no_open_requested(Some("true")));
        assert!(no_open_requested(Some("TRUE")));
    }

    #[test]
    fn linux_uses_xdg_open() {
        let command = opener_command("linux", false, "out.html");

        assert_eq!(command.program, "xdg-open");
        assert_eq!(command.args, vec!["out.html"]);
    }
}
