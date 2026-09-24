//! Cargo dependency policies for stable architecture boundaries.

use std::{collections::BTreeSet, path::Path};

use cargo_metadata::{DependencyKind, MetadataCommand, Package};

struct EdgePolicy {
    from: &'static str,
    label: &'static str,
    forbidden: &'static [&'static str],
    forbid_workspace_packages: bool,
    reason: &'static str,
}

const EDGE_POLICIES: [EdgePolicy; 11] = [
    EdgePolicy {
        from: "gtl-models",
        label: "gtl-models stays pure",
        forbidden: &[
            "gtl-application",
            "gtl-benchmarks",
            "gtl-wire",
            "gtl-infra",
            "gtl-artifacts",
            "gtl-cli",
            "gtl-client",
            "gtl-desktop",
            "gtl-local-transport",
            "gtl-parser",
            "gtl-server",
            "gtl-browser-e2e",
            "gtl-desktop-e2e",
            "xtask",
            "axum",
            "prost",
            "reqwest",
            "sqlx",
            "tonic",
            "tonic-health",
            "tonic-prost",
            "tonic-web",
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
            "gtl-client",
            "gtl-desktop",
            "gtl-local-transport",
            "gtl-server",
            "gtl-browser-e2e",
            "gtl-desktop-e2e",
            "xtask",
            "axum",
            "prost",
            "reqwest",
            "sqlx",
            "tonic",
            "tonic-health",
            "tonic-prost",
            "tonic-web",
        ],
        forbid_workspace_packages: false,
        reason: "use cases may depend on models and wire contracts, not adapters or process roots",
    },
    EdgePolicy {
        from: "gtl-wire",
        label: "gtl-wire stays transport-only",
        forbidden: &[
            "gtl-application",
            "gtl-benchmarks",
            "gtl-infra",
            "gtl-artifacts",
            "gtl-cli",
            "gtl-client",
            "gtl-desktop",
            "gtl-local-transport",
            "gtl-server",
            "gtl-browser-e2e",
            "gtl-desktop-e2e",
            "xtask",
            "axum",
            "reqwest",
            "sqlx",
        ],
        forbid_workspace_packages: false,
        reason: "wire DTOs may depend on pure model values, not use cases, adapters, frameworks, or process roots",
    },
    EdgePolicy {
        from: "gtl-local-transport",
        label: "gtl-local-transport stays IPC-only",
        forbidden: &[
            "gtl-application",
            "gtl-artifacts",
            "gtl-cli",
            "gtl-client",
            "gtl-desktop",
            "gtl-infra",
            "gtl-models",
            "gtl-server",
            "gtl-wire",
        ],
        forbid_workspace_packages: false,
        reason: "local IPC must not depend on domain or process behavior",
    },
    EdgePolicy {
        from: "gtl-client",
        label: "gtl-client stays transport-only",
        forbidden: &[
            "gtl-application",
            "gtl-artifacts",
            "gtl-cli",
            "gtl-desktop",
            "gtl-infra",
            "gtl-server",
        ],
        forbid_workspace_packages: false,
        reason: "the shared client may depend on wire and local IPC contracts, not application behavior or process roots",
    },
    EdgePolicy {
        from: "gtl-web",
        label: "gtl-web stays a presentation client",
        forbidden: &[
            "gtl-application",
            "gtl-desktop",
            "gtl-infra",
            "gtl-local-transport",
            "gtl-parser",
            "gtl-server",
            "prost",
            "tonic",
            "tonic-prost",
            "tonic-web",
            "tonic-web-wasm-client",
        ],
        forbid_workspace_packages: false,
        reason: "the WebView consumes typed gtl-client operations and must not own application, infrastructure, or protobuf transport behavior",
    },
    EdgePolicy {
        from: "gtl-server",
        label: "gtl-server delegates viewer projection",
        forbidden: &["gtl-parser"],
        forbid_workspace_packages: false,
        reason: "the process root must ask gtl-application to project diff rows instead of owning parser behavior",
    },
    EdgePolicy {
        from: "gtl-desktop",
        label: "gtl-desktop stays a transport shell",
        forbidden: &[
            "gtl-application",
            "gtl-artifacts",
            "gtl-infra",
            "gtl-models",
            "gtl-parser",
            "gtl-server",
        ],
        forbid_workspace_packages: false,
        reason: "the Tauri process may forward typed client operations but must not execute application or infrastructure behavior",
    },
    EdgePolicy {
        from: "gtl-cli",
        label: "gtl-cli stays a transport adapter",
        forbidden: &["gtl-application", "gtl-infra"],
        forbid_workspace_packages: false,
        reason: "the CLI must call gtl-server through gtl-client instead of executing application or infrastructure behavior",
    },
    EdgePolicy {
        from: "gtl-macros",
        label: "gtl-macros stays a leaf proc macro",
        forbidden: &[],
        forbid_workspace_packages: true,
        reason: "derive expansions name gtl-models paths; the proc macro itself must not depend on workspace packages",
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
        collect_package_violations(policy, package, &workspace_packages, &mut violations);
    }

    violations.sort();
    violations.dedup();
    Ok(violations)
}

fn collect_package_violations(
    policy: &EdgePolicy,
    package: &Package,
    workspace_packages: &BTreeSet<String>,
    violations: &mut Vec<String>,
) {
    for dependency in package.dependencies.iter().filter(|dependency| {
        matches!(
            dependency.kind,
            DependencyKind::Normal | DependencyKind::Development
        )
    }) {
        if dependency_is_forbidden(policy, &dependency.name, workspace_packages) {
            violations.push(format!(
                "[{}] {} -> {}: {}",
                policy.label, policy.from, dependency.name, policy.reason
            ));
        }
        if policy.from == "gtl-application"
            && dependency.name == "gtl-wire"
            && dependency_enables_generated_transport(
                dependency.uses_default_features,
                &dependency.features,
            )
        {
            violations.push(
                "[gtl-application omits generated transport] gtl-application -> gtl-wire protobuf/grpc: application code may use hand-written wire contracts but not generated protobuf or Tonic types"
                    .to_owned(),
            );
        }
    }
}

fn dependency_enables_generated_transport(
    uses_default_features: bool,
    features: &[String],
) -> bool {
    uses_default_features
        || features
            .iter()
            .any(|feature| matches!(feature.as_str(), "grpc" | "protobuf"))
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
            .unwrap()
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

        for dependency in ["similar", "tree-sitter", "tree-sitter-highlight"] {
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
            .unwrap();

        assert!(dependency_is_forbidden(
            models_policy,
            "reqwest",
            &BTreeSet::new()
        ));
    }

    #[test]
    fn wire_policy_accepts_models_and_rejects_application_behavior() {
        let wire_policy = EDGE_POLICIES
            .iter()
            .find(|policy| policy.from == "gtl-wire")
            .unwrap();

        assert!(!dependency_is_forbidden(
            wire_policy,
            "gtl-models",
            &BTreeSet::new()
        ));
        assert!(dependency_is_forbidden(
            wire_policy,
            "gtl-application",
            &BTreeSet::new()
        ));
    }

    #[test]
    fn client_policy_accepts_local_transport_and_wire_dependencies() {
        let client_policy = EDGE_POLICIES
            .iter()
            .find(|policy| policy.from == "gtl-client")
            .unwrap();

        for dependency in ["gtl-local-transport", "gtl-models", "gtl-wire", "tonic"] {
            assert!(!dependency_is_forbidden(
                client_policy,
                dependency,
                &BTreeSet::new()
            ));
        }
        assert!(dependency_is_forbidden(
            client_policy,
            "gtl-application",
            &BTreeSet::new()
        ));
    }

    #[test]
    fn cli_policy_rejects_application_and_infrastructure_dependencies() {
        let cli_policy = EDGE_POLICIES
            .iter()
            .find(|policy| policy.from == "gtl-cli")
            .unwrap();

        for dependency in ["gtl-client", "gtl-models", "gtl-wire"] {
            assert!(!dependency_is_forbidden(
                cli_policy,
                dependency,
                &BTreeSet::new()
            ));
        }
        for dependency in ["gtl-application", "gtl-infra"] {
            assert!(dependency_is_forbidden(
                cli_policy,
                dependency,
                &BTreeSet::new()
            ));
        }
    }

    #[test]
    fn application_wire_dependency_must_omit_generated_transport_and_default_features() {
        assert!(!dependency_enables_generated_transport(false, &[]));
        assert!(dependency_enables_generated_transport(true, &[]));
        assert!(dependency_enables_generated_transport(
            false,
            &["grpc".to_owned()]
        ));
        assert!(dependency_enables_generated_transport(
            false,
            &["protobuf".to_owned()]
        ));
    }

    #[test]
    fn web_policy_rejects_direct_grpc_dependencies() {
        let web_policy = EDGE_POLICIES
            .iter()
            .find(|policy| policy.from == "gtl-web")
            .unwrap();

        for dependency in [
            "tonic",
            "tonic-web-wasm-client",
            "gtl-application",
            "gtl-parser",
        ] {
            assert!(dependency_is_forbidden(
                web_policy,
                dependency,
                &BTreeSet::new()
            ));
        }
        for dependency in ["gtl-client", "gtl-models", "gtl-wire"] {
            assert!(!dependency_is_forbidden(
                web_policy,
                dependency,
                &BTreeSet::new()
            ));
        }
    }

    #[test]
    fn server_policy_rejects_direct_parser_dependencies() {
        let server_policy = EDGE_POLICIES
            .iter()
            .find(|policy| policy.from == "gtl-server")
            .unwrap();

        assert!(dependency_is_forbidden(
            server_policy,
            "gtl-parser",
            &BTreeSet::new()
        ));
        for dependency in ["gtl-application", "gtl-infra", "gtl-wire"] {
            assert!(!dependency_is_forbidden(
                server_policy,
                dependency,
                &BTreeSet::new()
            ));
        }
    }

    #[test]
    fn desktop_policy_accepts_transport_dependencies() {
        let desktop_policy = EDGE_POLICIES
            .iter()
            .find(|policy| policy.from == "gtl-desktop")
            .unwrap();

        for dependency in ["gtl-application", "gtl-infra", "gtl-models", "gtl-server"] {
            assert!(dependency_is_forbidden(
                desktop_policy,
                dependency,
                &BTreeSet::new()
            ));
        }
        for dependency in ["gtl-client", "gtl-wire", "tauri", "serde_json"] {
            assert!(!dependency_is_forbidden(
                desktop_policy,
                dependency,
                &BTreeSet::new()
            ));
        }
    }
}
