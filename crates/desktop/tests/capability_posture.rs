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
