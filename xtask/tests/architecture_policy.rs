#![cfg(test)]

use std::{fs, path::Path, process::Command};

#[test]
fn check_structure_rejects_application_dependency_on_infrastructure() {
    let workspace = tempfile::tempdir().expect("create temporary workspace");
    write_workspace(
        workspace.path(),
        &[
            (
                "gtl-application",
                "gtl-application",
                "[dependencies]\ngtl-infra = { path = \"../gtl-infra\" }\n",
            ),
            ("gtl-infra", "gtl-infra", ""),
        ],
    );

    let output = xtask(workspace.path()).output().expect("run xtask");

    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(
            "[gtl-application points inward] gtl-application -> gtl-infra: \
             use cases may depend on models and wire contracts, not adapters or process roots"
        ),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_structure_rejects_parser_dependency_on_a_workspace_package() {
    let workspace = tempfile::tempdir().expect("create temporary workspace");
    write_workspace(
        workspace.path(),
        &[
            (
                "gtl-parser",
                "gtl-parser",
                "[dependencies]\ngtl-models = { path = \"../gtl-models\" }\n",
            ),
            ("gtl-models", "gtl-models", ""),
        ],
    );

    let output = xtask(workspace.path()).output().expect("run xtask");

    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(
            "[gtl-parser stays reusable] gtl-parser -> gtl-models: \
             the reusable wasm parser must not depend on repository-specific packages"
        ),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_structure_allows_build_dependencies() {
    let workspace = tempfile::tempdir().expect("create temporary workspace");
    write_workspace(
        workspace.path(),
        &[
            (
                "gtl-application",
                "gtl-application",
                "[build-dependencies]\ngtl-infra = { path = \"../gtl-infra\" }\n",
            ),
            ("gtl-infra", "gtl-infra", ""),
            ("gtl-contracts", "gtl-contracts", ""),
        ],
    );

    let output = xtask(workspace.path()).output().expect("run xtask");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn xtask(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command
        .current_dir(root)
        .env("CARGO_NET_OFFLINE", "true")
        .arg("check-structure");
    command
}

fn write_workspace(root: &Path, packages: &[(&str, &str, &str)]) {
    let members = packages
        .iter()
        .map(|(directory, _, _)| format!("\"{directory}\""))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        root.join("Cargo.toml"),
        format!("[workspace]\nresolver = \"3\"\nmembers = [{members}]\n"),
    )
    .expect("write workspace manifest");

    for (directory, name, dependencies) in packages {
        let package = root.join(directory);
        fs::create_dir_all(package.join("src")).expect("create package source directory");
        fs::write(
            package.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
                 {dependencies}"
            ),
        )
        .expect("write package manifest");
        fs::write(package.join("src/lib.rs"), "").expect("write package source");
    }
}
