//! Rebuild tracked frontend assets, validate their presentation policy, and reject source drift.

use std::process::Command;

use anyhow::{Result, bail};

use super::{dioxus_web, frontend};
use crate::project;

/// Tracked generated paths paired with the command that regenerates them.
const BUNDLES: &[(&str, &str)] = &[
    (
        "crates/gtl-artifacts/src/embedded/generated/",
        "just cli build",
    ),
    ("crates/gtl-web/assets/generated/", "just web build"),
    ("crates/gtl-web/assets/tailwind.css", "just web styles"),
    ("crates/gtl-web/assets/diff-island.css", "just web styles"),
];

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

/// Rebuilds the bundle, validates its presentation policy, then diffs the committed output.
pub fn run() -> Result<()> {
    let root = project::repository_root();
    let _lock = project::lock_frontend_assets(&root)?;
    frontend::build_unlocked(&root)?;
    dioxus_web::build_styles_unlocked(&root)?;
    dioxus_web::verify_staged_bundle_if_present()?;
    super::presentation::run()?;
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
    fn check_drift_fails_with_rebuild_hint_when_a_bundle_is_dirty() {
        let err = check_drift(BUNDLES, &|_| false).unwrap_err().to_string();
        assert!(
            err.contains("crates/gtl-artifacts/src/embedded/generated/ is stale"),
            "{err}"
        );
        assert!(err.contains("just cli build"), "{err}");
    }

    #[test]
    fn check_drift_covers_the_tracked_dioxus_styles() {
        let err = check_drift(BUNDLES, &|path| {
            path != "crates/gtl-web/assets/diff-island.css"
        })
        .unwrap_err()
        .to_string();

        assert!(err.contains("assets/diff-island.css is stale"), "{err}");
        assert!(err.contains("just web styles"), "{err}");
    }

    #[test]
    fn check_drift_covers_the_tracked_diff_island_script() {
        let error = check_drift(BUNDLES, &|path| path != "crates/gtl-web/assets/generated/")
            .expect_err("generated diff-island script drift must fail")
            .to_string();

        assert!(error.contains("assets/generated/ is stale"), "{error}");
        assert!(error.contains("just web build"), "{error}");
    }
}
