//! `check-structure` verb — mechanical architecture lint.
//!
//! Walks every `crates/*/src` and `shared/*/src` directory and exits 3 on any violation of the
//! three layout rules (see `docs/planning/specs/2026-07-02-pragmatic-backend-architecture.md`,
//! Mechanical Gates section). Called standalone as `cargo run -p xtask -- check-structure
//! [<root>]` and wired
//! into `fmt-check` so the gate runs on every CI / pre-commit invocation.
//!
//! ## Rules
//!
//! 1. **Max folder depth 2** under each crate's `src/`. `src/a/b/` is the deepest allowed;
//!    `src/a/b/c/` is a violation. Depth is counted as directory components only (files do not
//!    count toward depth).
//! 2. **Feature folders are flat.** No directory named `errors` or `events` may be nested inside a
//!    feature folder (depth ≥ 2 under `src/`). The convention is flat files `errors.rs` /
//!    `events.rs` alongside the feature's handler — never a subdirectory.
//! 3. **No `services/` directory.** The convention is per-feature `service.rs` files plus a single
//!    `application/src/shared/` module; any directory named `services` anywhere in a crate's `src`
//!    tree is a violation.

use std::path::Path;

use anyhow::{Context as _, Result};

/// Walk every `crates/*/src` and `shared/*/src` directory under `root` and exit 3 on any
/// architecture violation.
///
/// `root` defaults to `.` (the current working directory, which should be the repo root). Pass an
/// explicit path when testing against a fixture tree.
///
/// # Errors
///
/// Returns an error when the filesystem cannot be traversed. Structural violations do **not**
/// produce an `Err` — they print to stderr and call [`std::process::exit`]`(3)`.
pub(crate) fn run(root: Option<&Path>) -> Result<()> {
    let root = root.unwrap_or(Path::new("."));
    let violations = collect_violations(root)
        .with_context(|| format!("check-structure: scanning {}", root.display()))?;

    for v in &violations {
        eprintln!("check-structure violation: {v}");
    }

    if !violations.is_empty() {
        eprintln!(
            "\n{} violation(s) — fix the directory structure and re-run `just fmt-check`",
            violations.len()
        );
        std::process::exit(3);
    }

    Ok(())
}

/// The workspace member directories scanned for layout violations — the app crates and the
/// app-agnostic `shared/*` crates. Both buckets obey the same three rules.
const MEMBER_DIRS: [&str; 2] = ["crates", "shared"];

/// Crates exempt from rule 1 (max folder depth) while they are still legacy-shaped —
/// rules 2 and 3 still apply. Shrink this list as migration phases land — new
/// architecture crates (`domain`, `contracts`, `application`, `infra`, `daemon`) are
/// never added.
const DEPTH_EXEMPT_CRATES: [&str; 1] = ["cli"];

/// Collect every violation under `root/{crates,shared}/*/src`.
fn collect_violations(root: &Path) -> Result<Vec<String>> {
    let mut violations = Vec::new();
    for member_dir in MEMBER_DIRS {
        collect_member_violations(&root.join(member_dir), &mut violations)?;
    }
    Ok(violations)
}

/// Collect violations under one member directory (e.g. `crates/` or `shared/`). A missing directory
/// is not an error — it simply contributes nothing.
fn collect_member_violations(member_dir: &Path, violations: &mut Vec<String>) -> Result<()> {
    if !member_dir.is_dir() {
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(member_dir)
        .with_context(|| format!("reading {}", member_dir.display()))?
        .collect::<Result<_, _>>()?;

    // Sort for deterministic output order in tests.
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let crate_name = entry.file_name();
        let skip_depth_rule = DEPTH_EXEMPT_CRATES.iter().any(|e| crate_name == *e);
        let src = entry.path().join("src");
        if !src.is_dir() {
            continue;
        }
        walk_dirs(&src, 0, skip_depth_rule, violations)?;
    }

    Ok(())
}

/// Recursively walk directories under `src/`, applying all three rules to each discovered
/// sub-directory.
///
/// `current` is the directory currently being iterated; `depth` is how many directory components
/// separate `current` from `src/` (0 = items directly inside `src/`). `skip_depth_rule` exempts a
/// legacy crate (see [`DEPTH_EXEMPT_CRATES`]) from rule 1 only — rules 2 and 3 still apply.
fn walk_dirs(
    current: &Path,
    depth: usize,
    skip_depth_rule: bool,
    violations: &mut Vec<String>,
) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(current)
        .with_context(|| format!("reading {}", current.display()))?
        .collect::<Result<_, _>>()?;

    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let child_depth = depth + 1;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Rule 1: max folder depth 2 under src/.
        if !skip_depth_rule && child_depth > 2 {
            violations.push(format!(
                "[rule 1: max depth 2] {} — depth {} under src/ (max is 2)",
                path.display(),
                child_depth,
            ));
            // Continue walking so deeper violations are also reported.
        }

        // Rule 2: no `errors/` or `events/` directory inside a feature folder (depth ≥ 2).
        if child_depth >= 2 && (name == "errors" || name == "events") {
            violations.push(format!(
                "[rule 2: feature folders flat — no nested errors/events dir] {}",
                path.display()
            ));
        }

        // Rule 3: no directory named `services` anywhere.
        if name == "services" {
            violations.push(format!(
                "[rule 3: no services/ dir] {} — use per-feature service.rs files instead",
                path.display()
            ));
        }

        walk_dirs(&path, child_depth, skip_depth_rule, violations)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn seed(base: &Path, rel: &str) {
        let full = base.join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, b"// fixture\n").unwrap();
    }

    #[test]
    fn compliant_repo_has_no_violations() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/domain/src/todos/item.rs");
        seed(dir.path(), "crates/api/src/endpoints/todos/create.rs");
        seed(dir.path(), "crates/domain/src/todos/errors.rs"); // file, not dir — fine
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.is_empty(), "unexpected violations: {v:?}");
    }

    #[test]
    fn depth_3_dir_is_a_rule_1_violation() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/x/src/a/b/c/f.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 1")), "violations: {v:?}");
    }

    #[test]
    fn nested_errors_dir_is_a_rule_2_violation() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/domain/src/todos/errors/detail.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 2")), "violations: {v:?}");
    }

    #[test]
    fn nested_events_dir_is_also_a_rule_2_violation() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/domain/src/todos/events/published.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 2")), "violations: {v:?}");
    }

    #[test]
    fn services_dir_is_a_rule_3_violation() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/app/src/services/foo.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 3")), "violations: {v:?}");
    }

    #[test]
    fn shared_member_dir_is_also_scanned() {
        // The relocated cqrs/bootstrap crates live under shared/, not crates/ — the lint must
        // reach them too. A depth-3 dir under a shared/ crate is a rule-1 violation.
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "shared/cqrs/src/a/b/c/f.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 1")), "violations: {v:?}");
    }

    #[test]
    fn no_member_dirs_returns_empty() {
        let dir = TempDir::new().unwrap();
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn real_repo_is_clean() {
        // CARGO_MANIFEST_DIR = xtask/; its parent is the workspace root where crates/ and
        // shared/ live. Anchoring here (not ".") makes the test actually scan crates/*
        // regardless of test CWD.
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask/ must have a parent (the workspace root)");
        let v = collect_violations(workspace_root).unwrap();
        assert!(
            v.is_empty(),
            "the real repo has check-structure violations:\n{}",
            v.join("\n")
        );
    }

    #[test]
    fn exempt_cli_crate_is_skipped() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/cli/src/commands/managed/status/mod.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.is_empty(), "cli must be exempt: {v:?}");
    }

    #[test]
    fn non_exempt_crate_with_same_shape_still_violates() {
        let dir = TempDir::new().unwrap();
        seed(
            dir.path(),
            "crates/infra/src/commands/managed/status/mod.rs",
        );
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 1")), "violations: {v:?}");
    }

    #[test]
    fn exempt_cli_crate_still_violates_rule_3() {
        // cli is exempt from rule 1 (max depth) only — a services/ dir must still be caught.
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/cli/src/services/foo.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 3")), "violations: {v:?}");
    }
}
