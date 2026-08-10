//! Cargo dependency policies for stable architecture boundaries.

use std::{collections::BTreeSet, path::Path};

use cargo_metadata::{DependencyKind, MetadataCommand};

struct EdgePolicy {
    from: &'static str,
    label: &'static str,
    forbidden: &'static [&'static str],
    forbid_workspace_packages: bool,
    reason: &'static str,
}

const EDGE_POLICIES: [EdgePolicy; 5] = [
    EdgePolicy {
        from: "gtl-models",
        label: "gtl-models stays pure",
        forbidden: &[
            "gtl-application",
            "gtl-benchmarks",
            "gtl-contracts",
            "gtl-infra",
            "gtl-artifacts",
            "gtl-cli",
            "gtl-daemon",
            "gtl-desktop",
            "gtl-browser-e2e",
            "gtl-desktop-e2e",
            "xtask",
            "axum",
            "reqwest",
            "maud",
            "sqlx",
        ],
        forbid_workspace_packages: false,
        reason: "models concepts must not depend on use cases, adapters, frameworks, or process roots",
    },
    EdgePolicy {
        from: "gtl-application",
        label: "gtl-application points inward",
        forbidden: &[
            "gtl-benchmarks",
            "gtl-infra",
            "gtl-artifacts",
            "gtl-cli",
            "gtl-daemon",
            "gtl-desktop",
            "gtl-browser-e2e",
            "gtl-desktop-e2e",
            "xtask",
            "axum",
            "reqwest",
            "maud",
            "sqlx",
        ],
        forbid_workspace_packages: false,
        reason: "use cases may depend on models and wire contracts, not adapters or process roots",
    },
    EdgePolicy {
        from: "gtl-contracts",
        label: "gtl-contracts stay wire-only",
        forbidden: &[
            "gtl-models",
            "gtl-application",
            "gtl-benchmarks",
            "gtl-infra",
            "gtl-artifacts",
            "gtl-cli",
            "gtl-daemon",
            "gtl-desktop",
            "gtl-browser-e2e",
            "gtl-desktop-e2e",
            "xtask",
            "axum",
            "reqwest",
            "maud",
            "sqlx",
        ],
        forbid_workspace_packages: false,
        reason: "wire DTOs must not depend on product behavior or runtime adapters",
    },
    EdgePolicy {
        from: "gtl-infra",
        label: "gtl-infra owns no templates",
        forbidden: &["maud"],
        forbid_workspace_packages: false,
        reason: "presentation belongs to crates/gtl-artifacts, not the adapter layer",
    },
    EdgePolicy {
        from: "gtl-parser",
        label: "gtl-parser stays reusable",
        forbidden: &[],
        forbid_workspace_packages: true,
        reason: "the reusable wasm parser must not depend on repository-specific packages",
    },
];

pub(crate) fn run(root: Option<&Path>) {
    let root = root.unwrap_or(Path::new("."));
    let violations = match collect_violations(root) {
        Ok(violations) => violations,
        Err(error) => {
            eprintln!("check-structure failed to load locked Cargo metadata: {error}");
            std::process::exit(3);
        }
    };

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

fn collect_violations(root: &Path) -> Result<Vec<String>, cargo_metadata::Error> {
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .other_options(vec!["--locked".to_string()])
        .exec()?;
    let workspace_packages = metadata
        .workspace_packages()
        .into_iter()
        .map(|package| package.name.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let mut violations = Vec::new();

    for policy in &EDGE_POLICIES {
        let Some(package) = metadata
            .workspace_packages()
            .into_iter()
            .find(|package| package.name == policy.from)
        else {
            continue;
        };
        for dependency in package.dependencies.iter().filter(|dependency| {
            matches!(
                dependency.kind,
                DependencyKind::Normal | DependencyKind::Development
            )
        }) {
            if dependency_is_forbidden(policy, &dependency.name, &workspace_packages) {
                violations.push(format!(
                    "[{}] {} -> {}: {}",
                    policy.label, policy.from, dependency.name, policy.reason
                ));
            }
        }
    }

    violations.sort();
    violations.dedup();
    Ok(violations)
}

fn dependency_is_forbidden(
    policy: &EdgePolicy,
    dependency: &str,
    workspace_packages: &BTreeSet<String>,
) -> bool {
    policy.forbidden.contains(&dependency)
        || (policy.forbid_workspace_packages
            && dependency != policy.from
            && workspace_packages.contains(dependency))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser_policy() -> &'static EdgePolicy {
        EDGE_POLICIES
            .iter()
            .find(|policy| policy.from == "gtl-parser")
            .expect("parser policy should exist")
    }

    #[test]
    fn parser_policy_rejects_workspace_packages() {
        let workspace_packages = BTreeSet::from(["gtl-models".to_string()]);

        assert!(dependency_is_forbidden(
            parser_policy(),
            "gtl-models",
            &workspace_packages
        ));
    }

    #[test]
    fn parser_policy_accepts_external_dependencies() {
        let workspace_packages = BTreeSet::from(["gtl-parser".to_string()]);

        for dependency in ["similar", "syntect", "thiserror"] {
            assert!(!dependency_is_forbidden(
                parser_policy(),
                dependency,
                &workspace_packages
            ));
        }
    }

    #[test]
    fn named_policy_still_rejects_framework_dependencies() {
        let models_policy = EDGE_POLICIES
            .iter()
            .find(|policy| policy.from == "gtl-models")
            .expect("models policy should exist");

        assert!(dependency_is_forbidden(
            models_policy,
            "maud",
            &BTreeSet::new()
        ));
    }
}
