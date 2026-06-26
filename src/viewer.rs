//! Pure viewer-launch decisions for the CLI, plus binary/url resolution. The
//! effectful launch lives in `commands::open_artifact`.
use std::path::{Path, PathBuf};

use crate::config::Viewer;

/// What the CLI should do with a freshly stored artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerAction {
    /// Spawn the desktop app on the `diff://` url.
    SpawnApp,
    /// Open the artifact file in the OS browser (from the store).
    Browser,
    /// Do nothing (path already printed).
    Nothing,
}

/// Decide the launch action. `no_open` (i.e. `GIT_TOOLS_NO_OPEN` is truthy) is
/// highest precedence — always `Nothing`. Otherwise `app` needs a display;
/// without one it degrades to the browser path (best-effort). Pure.
pub fn resolve_viewer_action(viewer: Viewer, has_display: bool, no_open: bool) -> ViewerAction {
    if no_open {
        return ViewerAction::Nothing;
    }
    match viewer {
        Viewer::None => ViewerAction::Nothing,
        Viewer::Browser => ViewerAction::Browser,
        Viewer::App if has_display => ViewerAction::SpawnApp,
        Viewer::App => ViewerAction::Browser,
    }
}

/// Returns true if the given env-var value is a truthy NO_OPEN sentinel
/// (`1 | true | TRUE | yes | YES`, after trimming). Mirrors gtl-platform's
/// `no_open_requested` but kept local to avoid a cross-crate dep for a
/// pure predicate.
pub(crate) fn is_no_open(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

/// `<store>/diffs/<repo-id>/<hash>.html` → `diff://<repo-id>/<hash>`. Pure.
pub fn diff_url_from_path(path: &Path) -> Option<String> {
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
    fn app_spawns_with_display_and_falls_back_without() {
        assert_eq!(
            resolve_viewer_action(Viewer::App, true, false),
            ViewerAction::SpawnApp
        );
        assert_eq!(
            resolve_viewer_action(Viewer::App, false, false),
            ViewerAction::Browser
        );
    }

    #[test]
    fn browser_and_none_ignore_display() {
        assert_eq!(
            resolve_viewer_action(Viewer::Browser, false, false),
            ViewerAction::Browser
        );
        assert_eq!(
            resolve_viewer_action(Viewer::None, true, false),
            ViewerAction::Nothing
        );
    }

    #[test]
    fn no_open_wins_over_app_and_browser() {
        assert_eq!(
            resolve_viewer_action(Viewer::App, true, true),
            ViewerAction::Nothing
        );
        assert_eq!(
            resolve_viewer_action(Viewer::Browser, true, true),
            ViewerAction::Nothing
        );
    }

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
            diff_url_from_path(p).as_deref(),
            Some("diff://0123456789abcdef/fedcba9876543210")
        );
    }
}
