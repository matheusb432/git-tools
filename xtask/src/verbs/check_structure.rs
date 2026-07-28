//! Cargo dependency policies for stable architecture boundaries.

use std::path::Path;

use cargo_metadata::{DependencyKind, MetadataCommand};

struct EdgePolicy {
    from: &'static str,
    label: &'static str,
    forbidden: &'static [&'static str],
    reason: &'static str,
}

const EDGE_POLICIES: [EdgePolicy; 5] = [
    EdgePolicy {
        from: "domain",
        label: "domain stays pure",
        forbidden: &[
            "application",
            "contracts",
            "infra",
            "preview",
            "cli",
            "daemon",
            "desktop",
            "browser-e2e",
            "desktop-e2e",
            "e2e",
            "xtask",
            "axum",
            "reqwest",
            "maud",
            "sqlx",
        ],
        reason: "domain concepts must not depend on use cases, adapters, frameworks, or process roots",
    },
    EdgePolicy {
        from: "application",
        label: "application points inward",
        forbidden: &[
            "infra",
            "preview",
            "cli",
            "daemon",
            "desktop",
            "browser-e2e",
            "desktop-e2e",
            "e2e",
            "xtask",
            "axum",
            "reqwest",
            "maud",
            "sqlx",
        ],
        reason: "use cases may depend on domain and wire contracts, not adapters or process roots",
    },
    EdgePolicy {
        from: "contracts",
        label: "contracts stay wire-only",
        forbidden: &[
            "domain",
            "application",
            "infra",
            "preview",
            "cli",
            "daemon",
            "desktop",
            "browser-e2e",
            "desktop-e2e",
            "e2e",
            "xtask",
            "axum",
            "reqwest",
            "maud",
            "sqlx",
        ],
        reason: "wire DTOs must not depend on product behavior or runtime adapters",
    },
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

fn collect_violations(root: &Path) -> Vec<String> {
    let Ok(metadata) = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .exec()
    else {
        return Vec::new();
    };
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
            if policy.forbidden.contains(&dependency.name.as_str()) {
                violations.push(format!(
                    "[{}] {} -> {}: {}",
                    policy.label, policy.from, dependency.name, policy.reason
                ));
            }
        }
    }

    violations.sort();
    violations.dedup();
    violations
}
