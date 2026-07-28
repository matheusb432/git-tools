//! `check-deps` verb -- dependency-direction lint.
//!
//! Reads the workspace's resolved package graph through `cargo_metadata` and exits 3 when the
//! spec's inward-pointing rules break:
//!
//! 1. A crate under `shared/` must never depend on an app crate (a path dependency whose resolved
//!    package lives under `crates/`).
//! 2. The core crates (`domain`, `application`, `contracts`) must never depend on the outer crates
//!    (`infra`, `daemon`, `cli`, `desktop`) nor on framework/adapter dependencies (`axum`,
//!    `reqwest`, `maud`, `sqlx`, `gtl-platform`). `contracts` additionally must not depend on
//!    `domain` or `application` (wire DTOs only).
//!
//! `cargo_metadata` flattens every dependency table a manifest can declare -- normal, dev,
//! build, and each `[target.<cfg>]` variant -- into one resolved list per package, so a
//! violation hiding behind a `cfg(...)` or a dev-only table cannot escape either rule.

use std::path::Path;

use cargo_metadata::{Dependency, DependencyKind, MetadataCommand, Package};

/// Exits 3 on any dependency-direction violation. Never returns `Err`: an unresolvable
/// workspace is ambiguous, not a violation (see [`collect_violations`]).
pub(crate) fn run(root: Option<&Path>) {
    let root = root.unwrap_or(Path::new("."));
    let violations = collect_violations(root);

    for v in &violations {
        eprintln!("check-deps violation: {v}");
    }

    if !violations.is_empty() {
        eprintln!(
            "\n{} violation(s) — dependencies must point inward (see docs/planning/specs/2026-07-02-pragmatic-backend-architecture.md)",
            violations.len()
        );
        std::process::exit(3);
    }
}

/// Dep names the core crates may never declare, plus (for `contracts`) the inner
/// crates it must not leak.
const CORE_BANNED: [&str; 9] = [
    "infra",
    "daemon",
    "cli",
    "desktop",
    "axum",
    "reqwest",
    "maud",
    "sqlx",
    "gtl-platform",
];
const CORE_CRATES: [&str; 3] = ["domain", "application", "contracts"];
const CONTRACTS_EXTRA_BANNED: [&str; 2] = ["domain", "application"];

/// Reads every workspace member's declared dependencies and applies both direction rules.
/// A workspace `cargo_metadata` cannot resolve (no root manifest, invalid layout) is
/// ambiguous, not a violation.
fn collect_violations(root: &Path) -> Vec<String> {
    let Ok(metadata) = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .exec()
    else {
        return Vec::new();
    };

    let shared_dir = root.join("shared");
    let crates_dir = root.join("crates");
    let mut violations = Vec::new();

    for package in &metadata.packages {
        if is_under(&package.manifest_path, &shared_dir) {
            check_shared_package(package, &crates_dir, &mut violations);
        } else if CORE_CRATES.contains(&package.name.as_str()) {
            check_core_package(package, &mut violations);
        }
    }

    violations.sort();
    violations
}

/// Rule 1: a `shared/*` package must not carry a path dependency resolving into `crates/`.
fn check_shared_package(package: &Package, crates_dir: &Path, violations: &mut Vec<String>) {
    for dependency in &package.dependencies {
        let Some(dep_path) = &dependency.path else {
            continue;
        };
        if is_under(dep_path, crates_dir) {
            violations.push(format!(
                "[rule 1: shared stays app-agnostic] {} [{}] depends on app crate `{}`",
                package.manifest_path,
                dependency_table(dependency),
                dependency.name,
            ));
        }
    }
}

/// Rule 2: a core crate must not depend on an outer crate or adapter framework, and
/// `contracts` additionally must not depend on `domain` or `application`.
fn check_core_package(package: &Package, violations: &mut Vec<String>) {
    for dependency in &package.dependencies {
        let name = dependency.name.as_str();
        let banned = CORE_BANNED.contains(&name)
            || (package.name == "contracts" && CONTRACTS_EXTRA_BANNED.contains(&name));
        if banned {
            violations.push(format!(
                "[rule 2: dependencies point inward] {} [{}] depends on `{name}`",
                package.manifest_path,
                dependency_table(dependency),
            ));
        }
    }
}

/// Names the manifest table a resolved dependency edge came from, e.g. `dependencies`,
/// `target.'cfg(unix)'.dev-dependencies`.
fn dependency_table(dependency: &Dependency) -> String {
    let table = match dependency.kind {
        DependencyKind::Normal | DependencyKind::Unknown => "dependencies",
        DependencyKind::Development => "dev-dependencies",
        DependencyKind::Build => "build-dependencies",
    };
    match &dependency.target {
        Some(target) => format!("target.'{target}'.{table}"),
        None => table.to_string(),
    }
}

fn is_under(candidate: &cargo_metadata::camino::Utf8Path, ancestor: &Path) -> bool {
    candidate.as_std_path().starts_with(ancestor)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::TempDir;

    use super::*;

    /// Writes a minimal real workspace member: a manifest plus an empty `src/lib.rs`, so
    /// `cargo metadata` can parse and resolve it without touching the network.
    fn member(base: &Path, rel: &str, name: &str, extra_manifest: &str) {
        let dir = base.join(rel);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{extra_manifest}"),
        )
        .unwrap();
        fs::write(dir.join("src/lib.rs"), "").unwrap();
    }

    fn workspace(base: &Path, members: &[&str]) {
        let list = members
            .iter()
            .map(|m| format!("\"{m}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            base.join("Cargo.toml"),
            format!("[workspace]\nresolver = \"2\"\nmembers = [{list}]\n"),
        )
        .unwrap();
    }

    #[test]
    fn clean_workspace_has_no_violations() {
        let dir = TempDir::new().unwrap();
        workspace(dir.path(), &["crates/application", "shared/bootstrap"]);
        member(dir.path(), "crates/application", "application", "");
        member(dir.path(), "shared/bootstrap", "bootstrap", "");

        let v = collect_violations(dir.path());
        assert!(v.is_empty(), "unexpected violations: {v:?}");
    }

    #[test]
    fn shared_crate_depending_on_app_crate_via_target_cfg_is_a_violation() {
        let dir = TempDir::new().unwrap();
        workspace(dir.path(), &["crates/application", "shared/bootstrap"]);
        member(dir.path(), "crates/application", "application", "");
        member(
            dir.path(),
            "shared/bootstrap",
            "bootstrap",
            "\n[target.'cfg(unix)'.dependencies]\napplication = { path = \"../../crates/application\" }\n",
        );

        let v = collect_violations(dir.path());
        assert!(
            v.iter()
                .any(|s| s.contains("shared/bootstrap") && s.contains("application")),
            "violations: {v:?}"
        );
    }

    #[test]
    fn core_crate_depending_outward_via_dev_dependencies_is_a_violation() {
        let dir = TempDir::new().unwrap();
        workspace(dir.path(), &["crates/domain", "crates/infra"]);
        member(
            dir.path(),
            "crates/domain",
            "domain",
            "\n[dev-dependencies]\ninfra = { path = \"../infra\" }\n",
        );
        member(dir.path(), "crates/infra", "infra", "");

        let v = collect_violations(dir.path());
        assert!(
            v.iter()
                .any(|s| s.contains("dev-dependencies") && s.contains("infra")),
            "violations: {v:?}"
        );
    }

    #[test]
    fn contracts_depending_on_domain_is_a_violation() {
        let dir = TempDir::new().unwrap();
        workspace(dir.path(), &["crates/contracts", "crates/domain"]);
        member(
            dir.path(),
            "crates/contracts",
            "contracts",
            "\n[dependencies]\ndomain = { path = \"../domain\" }\n",
        );
        member(dir.path(), "crates/domain", "domain", "");

        let v = collect_violations(dir.path());
        assert!(v.iter().any(|s| s.contains("domain")), "violations: {v:?}");
    }
}
