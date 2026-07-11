//! Viewer/display predicates shared by the CLI's render paths, plus the viewer
//! binary + `diff://` url resolvers. The effectful launch lives in
//! `commands::open_artifact` (browser — `--raw` and the headless degrade) and
//! `commands::open_in_viewer` (the default app path: spawn `gtl-viewer` on the
//! artifact's `diff://` url).
use std::path::{Path, PathBuf};

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

/// `<store>/diffs/<repo-id>/<hash>.html` → `diff://<repo-id>/<hash>`. The viewer
/// resolves this custom scheme back to the store artifact it loads in an iframe tab.
/// `None` for a path that isn't shaped like a store artifact. Pure.
pub fn diff_url_from_store_path(path: &Path) -> Option<String> {
    let hash = path.file_stem()?.to_str()?;
    let repo_id = path.parent()?.file_name()?.to_str()?;
    Some(format!("diff://{repo_id}/{hash}"))
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

    #[test]
    fn builds_diff_url_from_store_path() {
        let p = Path::new("/store/diffs/0123456789abcdef/fedcba9876543210.html");
        assert_eq!(
            diff_url_from_store_path(p).as_deref(),
            Some("diff://0123456789abcdef/fedcba9876543210")
        );
    }
}
