//! `xtask drift-check` — rebuild the committed frontend bundles and fail if they drift from
//! their TypeScript sources. Migrates the `_js-drift-guard` recipe. The bundle *build* stays bun
//! (reused via the existing `just cli build-js` / `just desktop build-viewer-ui` recipes — single
//! source for the vite invocation); this verb only orchestrates them and diffs the committed
//! output. Skipped with a message when bun is absent (the bundles can't be rebuilt to compare).

use std::process::Command;

use anyhow::{Result, bail};

use crate::proc;

/// The committed bundle dirs and the recipe that regenerates each, paired for the stale hint.
const BUNDLES: &[(&str, &str)] = &[
    ("crates/infra/src/embedded/generated/", "just cli build-js"),
    ("crates/desktop/dist/", "just desktop build-viewer-ui"),
];

/// Fail if any bundle dir has uncommitted changes after a rebuild — i.e. it drifted from its TS
/// source. `is_clean(dir)` reports whether the dir matches its committed state; injected so the
/// decision is host-testable without a real git tree.
fn check_drift(bundles: &[(&str, &str)], is_clean: &dyn Fn(&str) -> bool) -> Result<()> {
    for (dir, rebuild) in bundles {
        if !is_clean(dir) {
            bail!("{dir} is stale — run '{rebuild}' and commit");
        }
    }
    Ok(())
}

/// Whether `dir` has no uncommitted changes (`git diff --quiet` exits 0).
fn git_clean(dir: &str) -> bool {
    Command::new("git")
        .args(["diff", "--quiet", "--", dir])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Rebuild the bundles via the existing bun recipes, then diff the committed output.
pub fn run() -> Result<()> {
    if which::which("bun").is_err() {
        eprintln!("bun absent; skipping js drift guard");
        return Ok(());
    }
    proc::run("build-js", "just", &["cli", "build-js"])?;
    proc::run("build-viewer-ui", "just", &["desktop", "build-viewer-ui"])?;
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
        let err = check_drift(BUNDLES, &|dir| dir != "crates/desktop/dist/")
            .unwrap_err()
            .to_string();
        assert!(err.contains("crates/desktop/dist/ is stale"), "{err}");
        assert!(err.contains("just desktop build-viewer-ui"), "{err}");
    }
}
