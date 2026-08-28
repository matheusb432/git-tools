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
            ("gtl-wire", "gtl-wire", ""),
        ],
    );

    let output = xtask(workspace.path()).output().expect("run xtask");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_structure_rejects_generated_grpc_in_application() {
    let workspace = tempfile::tempdir().expect("create temporary workspace");
    write_workspace(
        workspace.path(),
        &[
            (
                "gtl-application",
                "gtl-application",
                "[dependencies]\ngtl-wire = { path = \"../gtl-wire\", default-features = false, features = [\"grpc\"] }\n",
            ),
            (
                "gtl-wire",
                "gtl-wire",
                "[features]\ndefault = []\ngrpc = []\n",
            ),
        ],
    );

    let output = xtask(workspace.path()).output().expect("run xtask");

    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("[gtl-application omits generated transport]"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_structure_rejects_direct_grpc_in_web() {
    let workspace = tempfile::tempdir().expect("create temporary workspace");
    write_workspace(
        workspace.path(),
        &[
            (
                "gtl-web",
                "gtl-web",
                "[dependencies]\ntonic = { path = \"../tonic\" }\n",
            ),
            ("tonic", "tonic", ""),
        ],
    );

    let output = xtask(workspace.path()).output().expect("run xtask");

    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("[gtl-web stays a presentation client] gtl-web -> tonic"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_structure_rejects_direct_parser_use_in_server() {
    let workspace = tempfile::tempdir().expect("create temporary workspace");
    write_workspace(
        workspace.path(),
        &[
            (
                "gtl-server",
                "gtl-server",
                "[dependencies]\ngtl-parser = { path = \"../gtl-parser\" }\n",
            ),
            ("gtl-parser", "gtl-parser", ""),
        ],
    );

    let output = xtask(workspace.path()).output().expect("run xtask");

    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("[gtl-server delegates viewer projection] gtl-server -> gtl-parser"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_structure_rejects_application_behavior_in_desktop() {
    let workspace = tempfile::tempdir().expect("create temporary workspace");
    write_workspace(
        workspace.path(),
        &[
            (
                "gtl-desktop",
                "gtl-desktop",
                "[dependencies]\ngtl-application = { path = \"../gtl-application\" }\n",
            ),
            ("gtl-application", "gtl-application", ""),
        ],
    );

    let output = xtask(workspace.path()).output().expect("run xtask");

    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("[gtl-desktop stays a transport shell] gtl-desktop -> gtl-application"),
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
