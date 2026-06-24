//! Security posture tripwire (ADR-0002 / tauri-app-config): the viewer must never
//! grant filesystem or shell capabilities — store access goes through the scoped
//! `diff://` scheme only (ADR-0004).
use std::fs;
use std::path::Path;

#[test]
fn capability_allowlist_grants_no_fs_or_shell() {
    let cap =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json"))
            .expect("capabilities/default.json must exist");
    let json: serde_json::Value = serde_json::from_str(&cap).expect("valid JSON");
    let perms = json["permissions"].as_array().expect("permissions array");
    for p in perms {
        let p = p.as_str().unwrap_or("");
        assert!(
            !p.starts_with("fs:") && !p.starts_with("shell:"),
            "forbidden capability in allowlist: {p}"
        );
    }
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
