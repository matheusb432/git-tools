//! Rebuild tracked web assets and reject source drift.

use std::process::Command;

use anyhow::{Result, bail};

use super::dioxus_web;
use crate::project;

/// Tracked generated paths paired with the command that regenerates them.
const BUNDLES: &[(&str, &str)] = &[("crates/gtl-web/assets/tailwind.css", "just web styles")];

/// Fail if any generated path has checkout changes after a rebuild. `is_clean(path)` reports
/// whether the path matches its committed state; injected so the
/// decision is host-testable without a real git tree.
fn check_drift(bundles: &[(&str, &str)], is_clean: &dyn Fn(&str) -> bool) -> Result<()> {
    for (dir, rebuild) in bundles {
        if !is_clean(dir) {
            bail!("{dir} is stale -- run '{rebuild}' and commit");
        }
    }
    Ok(())
}

/// Whether `path` has no tracked or untracked checkout changes.
fn git_clean(path: &str) -> bool {
    Command::new("git")
        .args(["status", "--short", "--untracked-files=all", "--", path])
        .current_dir(project::repository_root())
        .output()
        .is_ok_and(|output| output.status.success() && output.stdout.is_empty())
}

/// Rebuilds the web assets, then diffs the committed output.
pub fn run() -> Result<()> {
    let root = project::repository_root();
    let _lock = project::lock_web_assets(&root)?;
    dioxus_web::build_release_unlocked(&root)?;
    check_drift(BUNDLES, &git_clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_drift_passes_when_all_clean() {
        assert!(check_drift(BUNDLES, &|_| true).is_ok());
    }

    #[test]
    fn check_drift_fails_with_rebuild_hint_when_tailwind_is_dirty() {
        let err = check_drift(BUNDLES, &|_| false).unwrap_err().to_string();

        assert!(err.contains("assets/tailwind.css is stale"), "{err}");
        assert!(err.contains("just web styles"), "{err}");
    }
}
