//! `check-deps` verb — dependency-direction lint.
//!
//! Parses every `crates/*/Cargo.toml` and `shared/*/Cargo.toml` and exits 3 when the
//! spec's inward-pointing rules break:
//!
//! 1. A crate under `shared/` must never depend on an app crate (any `path` dep that resolves into
//!    `crates/`).
//! 2. The core crates (`domain`, `application`, `contracts`) must never depend on the outer crates
//!    (`infra`, `daemon`, `cli`, `desktop`) nor on framework/adapter dependencies (`axum`,
//!    `reqwest`, `maud`, `sqlx`, `gtl-platform`). `contracts` additionally must not depend on
//!    `domain` or `application` (wire DTOs only).

use std::path::Path;

use anyhow::{Context as _, Result};

pub(crate) fn run(root: Option<&Path>) -> Result<()> {
    let root = root.unwrap_or(Path::new("."));
    let violations = collect_violations(root)
        .with_context(|| format!("check-deps: scanning {}", root.display()))?;

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

    Ok(())
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

/// Whether a shared crate's relative `path` dependency resolves into the workspace's
/// top-level `crates/` directory. Lexical only — dep paths in scanned manifests are
/// relative to `shared/<crate>/`, so an app-crate dep must climb out (`../../`) and
/// re-enter through a literal `crates` component.
fn path_enters_crates(path: &str) -> bool {
    let mut stack: Vec<&str> = vec!["shared", "crate"];
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    stack.first().copied() == Some("crates")
}

fn collect_violations(root: &Path) -> Result<Vec<String>> {
    let mut violations = Vec::new();
    scan_member_dir(&root.join("shared"), true, &mut violations)?;
    scan_member_dir(&root.join("crates"), false, &mut violations)?;
    Ok(violations)
}

/// Scan one member dir (`shared/` or `crates/`); `is_shared` selects the rule set.
fn scan_member_dir(member_dir: &Path, is_shared: bool, violations: &mut Vec<String>) -> Result<()> {
    if !member_dir.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(member_dir)
        .with_context(|| format!("reading {}", member_dir.display()))?
        .collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let manifest_path = entry.path().join("Cargo.toml");
        if !manifest_path.is_file() {
            continue;
        }
        let crate_name = entry.file_name().to_string_lossy().into_owned();
        let manifest_str = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let manifest: toml::Value = toml::from_str(&manifest_str)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;
        let Some(deps) = manifest.get("dependencies").and_then(toml::Value::as_table) else {
            continue;
        };

        for (dep_name, dep_value) in deps {
            let path_dep = dep_value.get("path").and_then(toml::Value::as_str);
            if is_shared {
                // Rule 1: shared/* never depends on an app crate.
                if path_dep.is_some_and(path_enters_crates) {
                    violations.push(format!(
                        "[rule 1: shared stays app-agnostic] {} depends on app crate `{dep_name}`",
                        manifest_path.display(),
                    ));
                }
            } else if CORE_CRATES.contains(&crate_name.as_str()) {
                // Rule 2: core crates never depend outward.
                let effective_name = dep_value
                    .get("package")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(dep_name);
                let banned = CORE_BANNED.contains(&effective_name)
                    || (crate_name == "contracts"
                        && CONTRACTS_EXTRA_BANNED.contains(&effective_name));
                if banned {
                    let offender = if effective_name == dep_name {
                        dep_name.clone()
                    } else {
                        format!("{dep_name} (package = \"{effective_name}\")")
                    };
                    violations.push(format!(
                        "[rule 2: dependencies point inward] {} depends on `{offender}`",
                        manifest_path.display(),
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::TempDir;

    use super::*;

    fn manifest(base: &Path, rel: &str, body: &str) {
        let full = base.join(rel).join("Cargo.toml");
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, body).unwrap();
    }

    #[test]
    fn clean_workspace_has_no_violations() {
        let dir = TempDir::new().unwrap();
        manifest(
            dir.path(),
            "crates/application",
            "[package]\nname = \"application\"\n[dependencies]\ncqrs = { path = \"../../shared/cqrs\" }\ndomain = { path = \"../domain\" }\n",
        );
        manifest(
            dir.path(),
            "shared/cqrs",
            "[package]\nname = \"cqrs\"\n[dependencies]\nserde = \"1\"\n",
        );
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.is_empty(), "unexpected violations: {v:?}");
    }

    #[test]
    fn shared_crate_depending_on_app_crate_is_a_violation() {
        let dir = TempDir::new().unwrap();
        manifest(
            dir.path(),
            "shared/bootstrap",
            "[package]\nname = \"bootstrap\"\n[dependencies]\napplication = { path = \"../../crates/application\" }\n",
        );
        let v = collect_violations(dir.path()).unwrap();
        assert!(
            v.iter()
                .any(|s| s.contains("shared/bootstrap") && s.contains("application")),
            "violations: {v:?}"
        );
    }

    #[test]
    fn application_depending_on_infra_is_a_violation() {
        let dir = TempDir::new().unwrap();
        manifest(
            dir.path(),
            "crates/application",
            "[package]\nname = \"application\"\n[dependencies]\ninfra = { path = \"../infra\" }\n",
        );
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("infra")), "violations: {v:?}");
    }

    #[test]
    fn domain_depending_on_a_framework_is_a_violation() {
        let dir = TempDir::new().unwrap();
        manifest(
            dir.path(),
            "crates/domain",
            "[package]\nname = \"domain\"\n[dependencies]\naxum = \"0.8\"\n",
        );
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("axum")), "violations: {v:?}");
    }

    #[test]
    fn contracts_depending_on_domain_is_a_violation() {
        let dir = TempDir::new().unwrap();
        manifest(
            dir.path(),
            "crates/contracts",
            "[package]\nname = \"contracts\"\n[dependencies]\ndomain = { path = \"../domain\" }\n",
        );
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("domain")), "violations: {v:?}");
    }

    #[test]
    fn outer_crates_may_depend_on_anything_inward() {
        let dir = TempDir::new().unwrap();
        manifest(
            dir.path(),
            "crates/daemon",
            "[package]\nname = \"daemon\"\n[dependencies]\naxum = \"0.8\"\ninfra = { path = \"../infra\" }\napplication = { path = \"../application\" }\n",
        );
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.is_empty(), "unexpected violations: {v:?}");
    }

    #[test]
    fn shared_crate_sibling_dir_containing_crates_substring_is_not_a_violation() {
        let dir = TempDir::new().unwrap();
        manifest(
            dir.path(),
            "shared/bootstrap",
            "[package]\nname = \"bootstrap\"\n[dependencies]\nbar = { path = \"../other-crates/bar\" }\n",
        );
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.is_empty(), "false positive: {v:?}");
    }

    #[test]
    fn renamed_package_dep_on_banned_crate_is_a_violation() {
        let dir = TempDir::new().unwrap();
        manifest(
            dir.path(),
            "crates/domain",
            "[package]\nname = \"domain\"\n[dependencies]\nfoo = { package = \"axum\", version = \"0.8\" }\n",
        );
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("axum")), "violations: {v:?}");
    }
}
