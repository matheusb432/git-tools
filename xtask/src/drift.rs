//! `xtask drift-check` — rebuild the committed frontend bundle and fail if it drifts from
//! its TypeScript sources, then diffs the committed output.

use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::frontend;

/// The committed bundle dirs and the recipe that regenerates each, paired for the stale hint.
const BUNDLES: &[(&str, &str)] = &[("crates/infra/src/embedded/generated/", "just cli build")];

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
        .is_ok_and(|s| s.success())
}

/// Rebuilds the bundle via the existing Deno recipe, then diffs the committed output.
pub fn run() -> Result<()> {
    which::which("deno").context(
        "required tool `deno` is missing; install it through the declarative host configuration",
    )?;
    frontend::build()?;
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
            err.contains("crates/infra/src/embedded/generated/ is stale"),
            "{err}"
        );
        assert!(err.contains("just cli build"), "{err}");
    }
}
