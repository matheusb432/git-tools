#![cfg(test)]

//! Security posture tripwire for the custom-origin htmx viewer.
use std::{fs, path::Path};

#[test]
fn capability_allowlist_grants_no_fs_or_shell() {
    let cap =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json"))
            .expect("capabilities/default.json must exist");
    let json: serde_json::Value = serde_json::from_str(&cap).expect("valid JSON");
    assert_eq!(
        json["permissions"],
        serde_json::json!([
            "core:default",
            "core:window:allow-show",
            "core:window:allow-hide",
            "core:window:allow-set-focus"
        ]),
        "permissions must remain exactly the reviewed core/window set"
    );
}

#[test]
fn main_window_is_not_declared_or_backed_by_frontend_dist() {
    let conf = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json"))
        .expect("tauri.conf.json must exist");
    let json: serde_json::Value = serde_json::from_str(&conf).expect("valid JSON");

    assert!(json["build"]["frontendDist"].is_null());
    assert!(json["app"]["windows"].is_null());
}

#[test]
fn csp_is_null_for_offline_own_content() {
    let conf = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json"))
        .expect("tauri.conf.json must exist");
    let json: serde_json::Value = serde_json::from_str(&conf).expect("valid JSON");
    assert!(
        json["app"]["security"]["csp"].is_null(),
        "csp must be null (justified offline own-content)"
    );
}

#[test]
fn development_flavor_has_an_independent_single_instance_identity() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let release_conf = fs::read_to_string(manifest_dir.join("tauri.conf.json"))
        .expect("tauri.conf.json must exist");
    let development_conf = fs::read_to_string(manifest_dir.join("tauri.dev.conf.json"))
        .expect("tauri.dev.conf.json must exist");
    let release: serde_json::Value = serde_json::from_str(&release_conf).expect("valid JSON");
    let development: serde_json::Value =
        serde_json::from_str(&development_conf).expect("valid JSON");

    assert_eq!(release["identifier"], "dev.gittools.viewer");
    assert_eq!(development["identifier"], "dev.gittools.viewer.dev");
    assert_ne!(development["identifier"], release["identifier"]);
}
