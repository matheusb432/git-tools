//! Viewer/display predicates shared by the CLI's render paths, plus viewer binary
//! resolution for recipe forwarding.
use std::path::PathBuf;

/// Whether a display server is available (`DISPLAY` or `WAYLAND_DISPLAY` set,
/// non-empty presence — the *value* doesn't matter). Shared by the `--raw`/headless
/// routing decision (`commands::diff`, `commands::diff_subrepos`) and by
/// `open_artifact`'s degrade path.
pub fn has_display() -> bool {
    std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Returns true if the given env-var value is a truthy `NO_OPEN` sentinel
/// (`1 | true | TRUE | yes | YES`, after trimming). Mirrors gtl-platform's
/// `no_open_requested` but kept local to avoid a cross-crate dep for a
/// pure predicate.
pub(crate) fn is_no_open(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

pub(crate) fn no_open_requested() -> bool {
    is_no_open(std::env::var("GIT_TOOLS_NO_OPEN").ok().as_deref())
}

/// The viewer binary: `gtl-viewer` next to the running CLI exe, else on PATH.
/// `None` if neither exists (caller degrades to the browser path).
pub fn resolve_viewer_bin() -> Option<PathBuf> {
    let name = format!("gtl-viewer{}", std::env::consts::EXE_SUFFIX);
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join(&name);
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    // PATH fallback: rely on the OS resolver by returning the bare name if any
    // dir on PATH holds it.
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(&name))
            .find(|c| c.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_no_open_accepts_truthy_values() {
        assert!(is_no_open(Some("1")));
        assert!(is_no_open(Some("true")));
        assert!(is_no_open(Some("TRUE")));
        assert!(is_no_open(Some("yes")));
        assert!(is_no_open(Some("YES")));
        assert!(!is_no_open(Some("0")));
        assert!(!is_no_open(Some("false")));
        assert!(!is_no_open(None));
    }
}
