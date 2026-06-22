//! Pure, host-testable OS decisions. Parameterized by `Os`; no `#[cfg]`, no I/O.
use crate::Os;

/// The program + argv to open a file in the OS default handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenerCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// Decide the opener command for an OS. WSL and Windows both shell out to
/// `explorer.exe`; macOS uses `open`; everything else uses `xdg-open`.
pub fn opener_command(os: Os, is_wsl: bool, path: &str) -> OpenerCommand {
    if is_wsl || os == Os::Windows {
        return OpenerCommand { program: "explorer.exe".into(), args: vec![path.into()] };
    }
    match os {
        Os::Macos => OpenerCommand { program: "open".into(), args: vec![path.into()] },
        _ => OpenerCommand { program: "xdg-open".into(), args: vec![path.into()] },
    }
}

/// How a child is detached from the launching process, per OS family. The
/// effectful applier in `<os>/spawn.rs` turns this into a real spawn flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetachStrategy {
    /// Unix: own process group (`process_group(0)`) + null stdio.
    Unix,
    /// Windows: `DETACHED_PROCESS | CREATE_NO_WINDOW` (wired in Phase 3).
    Windows,
}

/// Decide the detach strategy for an OS. Pure; total over every `Os`.
pub fn detach_strategy(os: Os) -> DetachStrategy {
    match os {
        Os::Windows => DetachStrategy::Windows,
        Os::Linux | Os::Macos => DetachStrategy::Unix,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wsl_uses_windows_explorer_before_linux_fallback() {
        let c = opener_command(Os::Linux, true, "out.html");
        assert_eq!(c.program, "explorer.exe");
        assert_eq!(c.args, vec!["out.html"]);
    }

    #[test]
    fn macos_uses_open() {
        assert_eq!(opener_command(Os::Macos, false, "o.html").program, "open");
    }

    #[test]
    fn windows_passes_metacharacter_paths_as_one_argument() {
        let c = opener_command(Os::Windows, false, r"C:\tmp\a&b.html");
        assert_eq!(c.program, "explorer.exe");
        assert_eq!(c.args, vec![r"C:\tmp\a&b.html"]);
    }

    #[test]
    fn linux_uses_xdg_open() {
        assert_eq!(opener_command(Os::Linux, false, "o.html").program, "xdg-open");
    }

    #[test]
    fn windows_with_wsl_flag_short_circuits_to_explorer() {
        // is_wsl || os == Os::Windows → explorer.exe regardless of which triggered it.
        let c = opener_command(Os::Windows, true, "x.html");
        assert_eq!(c.program, "explorer.exe");
        assert_eq!(c.args, vec!["x.html"]);
    }

    #[test]
    fn detach_strategy_is_unix_for_linux_and_macos() {
        assert_eq!(detach_strategy(Os::Linux), DetachStrategy::Unix);
        assert_eq!(detach_strategy(Os::Macos), DetachStrategy::Unix);
    }

    #[test]
    fn detach_strategy_is_windows_for_windows() {
        assert_eq!(detach_strategy(Os::Windows), DetachStrategy::Windows);
    }
}
