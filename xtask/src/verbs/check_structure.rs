//! `check-structure` verb -- architecture forbidden-edge lint.
//!
//! Reads the workspace package graph through `cargo_metadata` and exits 3 when a package declares a
//! forbidden dependency edge. Two invariants hold:
//!
//! 1. `infra` must not depend on `maud`: presentation belongs to `crates/preview` and the
//!    process-root shells, not the adapter layer.
//! 2. `e2e` must not depend on any product implementation crate: black-box tests use wire contracts
//!    and app-agnostic shared crates only.
//!
//! Build-dependency edges are ignored: a build-script dependency is a code-generation concern, not
//! the shipped or test-surface direction these invariants govern. Semantic judgments -- thin
//! handlers, app-state purity, `AppStateStore` export surface, htmx presentation strings, and
//! source layout -- are owned by review, not this gate. A workspace whose metadata cannot resolve
//! is ambiguous, so it produces no violations.

use std::{collections::BTreeSet, path::Path};

use cargo_metadata::{DependencyKind, MetadataCommand};

/// One package that may not depend on a fixed set of other packages.
struct EdgePolicy {
    /// The package name whose declared dependencies are checked.
    from: &'static str,
    /// Short bracketed label leading each diagnostic for this policy.
    label: &'static str,
    /// The dependency names `from` may never declare.
    forbidden: &'static [&'static str],
    /// The rationale appended to each diagnostic for this policy.
    reason: &'static str,
}

/// The forbidden outward edges the workspace must never declare.
const EDGE_POLICIES: [EdgePolicy; 2] = [
    EdgePolicy {
        from: "infra",
        label: "infra owns no templates",
        forbidden: &["maud"],
        reason: "presentation belongs to crates/preview, not the adapter layer",
    },
    EdgePolicy {
        from: "e2e",
        label: "e2e stays black-box",
        forbidden: &[
            "application",
            "cli",
            "daemon",
            "desktop",
            "domain",
            "infra",
            "preview",
        ],
        reason: "black-box tests may use contracts and app-agnostic shared crates, not product internals",
    },
];

/// Exits 3 on any forbidden dependency edge. Never returns `Err`: an unresolvable workspace is
/// ambiguous, not a violation.
pub(crate) fn run(root: Option<&Path>) {
    let root = root.unwrap_or(Path::new("."));
    let violations = collect_violations(root);

    for violation in &violations {
        eprintln!("check-structure violation: {violation}");
    }

    if !violations.is_empty() {
        eprintln!(
            "\n{} architecture violation(s): fix the reported violations and re-run `just check`",
            violations.len()
        );
        std::process::exit(3);
    }
}

/// Reads the workspace package graph and reports every declared forbidden edge, sorted for
/// deterministic output.
fn collect_violations(root: &Path) -> Vec<String> {
    let Ok(metadata) = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .exec()
    else {
        return Vec::new();
    };

    let mut violations = Vec::new();
    for package in &metadata.packages {
        let Some(policy) = EDGE_POLICIES
            .iter()
            .find(|policy| policy.from == package.name.as_str())
        else {
            continue;
        };

        let forbidden_targets = package
            .dependencies
            .iter()
            .filter(|dependency| dependency.kind != DependencyKind::Build)
            .map(|dependency| dependency.name.as_str())
            .filter(|name| policy.forbidden.contains(name))
            .collect::<BTreeSet<_>>();

        for target in forbidden_targets {
            violations.push(format!(
                "[{}] crates/{} depends on {target}: {}",
                policy.label, policy.from, policy.reason
            ));
        }
    }

    violations.sort();
    violations
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::TempDir;

    use super::*;

    /// Writes a minimal real workspace member: a manifest plus an empty `src/lib.rs`, so
    /// `cargo metadata` can parse it without touching the network.
    fn member(base: &Path, rel: &str, name: &str, extra_manifest: &str) {
        let dir = base.join(rel);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{extra_manifest}"
            ),
        )
        .unwrap();
        fs::write(dir.join("src/lib.rs"), "").unwrap();
    }

    fn workspace(base: &Path, members: &[&str]) {
        let list = members
            .iter()
            .map(|member| format!("\"{member}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            base.join("Cargo.toml"),
            format!("[workspace]\nresolver = \"2\"\nmembers = [{list}]\n"),
        )
        .unwrap();
    }

    #[test]
    fn allowed_graph_has_no_violations() {
        let dir = TempDir::new().unwrap();
        workspace(
            dir.path(),
            &["crates/e2e", "crates/contracts", "crates/infra"],
        );
        member(
            dir.path(),
            "crates/e2e",
            "e2e",
            "\n[dev-dependencies]\ncontracts = { path = \"../contracts\" }\n",
        );
        member(dir.path(), "crates/contracts", "contracts", "");
        member(dir.path(), "crates/infra", "infra", "");

        let violations = collect_violations(dir.path());
        assert!(
            violations.is_empty(),
            "unexpected violations: {violations:?}"
        );
    }

    #[test]
    fn e2e_product_dependency_is_reported_with_its_diagnostic() {
        let dir = TempDir::new().unwrap();
        workspace(dir.path(), &["crates/e2e", "crates/infra"]);
        member(
            dir.path(),
            "crates/e2e",
            "e2e",
            "\n[dev-dependencies]\ninfra = { path = \"../infra\" }\n",
        );
        member(dir.path(), "crates/infra", "infra", "");

        let violations = collect_violations(dir.path());

        assert_eq!(
            violations,
            vec![
                "[e2e stays black-box] crates/e2e depends on infra: black-box tests may use contracts and app-agnostic shared crates, not product internals"
            ]
        );
    }

    #[test]
    fn build_dependency_edge_is_ignored() {
        let dir = TempDir::new().unwrap();
        workspace(dir.path(), &["crates/e2e", "crates/infra"]);
        member(
            dir.path(),
            "crates/e2e",
            "e2e",
            "\n[build-dependencies]\ninfra = { path = \"../infra\" }\n",
        );
        member(dir.path(), "crates/infra", "infra", "");

        let violations = collect_violations(dir.path());
        assert!(
            violations.is_empty(),
            "unexpected violations: {violations:?}"
        );
    }

    #[test]
    fn real_workspace_has_no_forbidden_edges() {
        // CARGO_MANIFEST_DIR = xtask/; its parent is the workspace root where crates/ and shared/
        // live. Anchoring here (not ".") makes the test scan the real graph regardless of test CWD.
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask/ must have a parent (the workspace root)");
        let violations = collect_violations(workspace_root);
        assert!(
            violations.is_empty(),
            "the real workspace has check-structure violations:\n{}",
            violations.join("\n")
        );
    }
}
