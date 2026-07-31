//! `xtask drift-check` rebuilds the committed frontend bundle, validates its presentation policy,
//! and fails if the generated output drifts from its sources.

use std::process::Command;

use anyhow::{Context, Result, bail};

use super::frontend;

/// The committed bundle dirs and the recipe that regenerates each, paired for the stale hint.
const BUNDLES: &[(&str, &str)] = &[
    ("crates/preview/src/embedded/generated/", "just cli build"),
    ("crates/desktop/src/embedded/generated/", "just cli build"),
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
        .is_ok_and(|s| s.success())
}

/// Rebuilds the bundle, validates its presentation policy, then diffs the committed output.
pub fn run() -> Result<()> {
    which::which("deno").context("required tool `deno` is missing; run `mise install deno`")?;
    frontend::build()?;
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
            err.contains("crates/preview/src/embedded/generated/ is stale"),
            "{err}"
        );
        assert!(err.contains("just cli build"), "{err}");
    }
}
