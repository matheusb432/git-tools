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
        from: "gtl-models",
        label: "gtl-models stays pure",
        forbidden: &[
            "gtl-application",
            "gtl-benchmarks",
            "gtl-contracts",
            "gtl-infra",
            "gtl-preview",
            "gtl-cli",
            "gtl-daemon",
            "gtl-desktop",
            "gtl-browser-e2e",
            "gtl-desktop-e2e",
            "gtl-e2e",
            "xtask",
            "axum",
            "reqwest",
            "maud",
            "sqlx",
        ],
        reason: "models concepts must not depend on use cases, adapters, frameworks, or process roots",
    },
    EdgePolicy {
        from: "gtl-application",
        label: "gtl-application points inward",
        forbidden: &[
            "gtl-benchmarks",
            "gtl-infra",
            "gtl-preview",
            "gtl-cli",
            "gtl-daemon",
            "gtl-desktop",
            "gtl-browser-e2e",
            "gtl-desktop-e2e",
            "gtl-e2e",
            "xtask",
            "axum",
            "reqwest",
            "maud",
            "sqlx",
        ],
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
            "gtl-preview",
            "gtl-cli",
            "gtl-daemon",
            "gtl-desktop",
            "gtl-browser-e2e",
            "gtl-desktop-e2e",
            "gtl-e2e",
            "xtask",
            "axum",
            "reqwest",
            "maud",
            "sqlx",
        ],
        reason: "wire DTOs must not depend on product behavior or runtime adapters",
    },
    EdgePolicy {
        from: "gtl-infra",
        label: "gtl-infra owns no templates",
        forbidden: &["maud"],
        reason: "presentation belongs to crates/gtl-preview, not the adapter layer",
    },
    EdgePolicy {
        from: "gtl-e2e",
        label: "gtl-e2e stays black-box",
        forbidden: &[
            "gtl-application",
            "gtl-benchmarks",
            "gtl-cli",
            "gtl-daemon",
            "gtl-desktop",
            "gtl-models",
            "gtl-infra",
            "gtl-preview",
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
