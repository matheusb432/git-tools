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

#[test]
fn production_flavor_embeds_the_local_dioxus_bundle() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let production_conf = fs::read_to_string(manifest_dir.join("tauri.production.conf.json"))
        .expect("tauri.production.conf.json must exist");
    let production: serde_json::Value = serde_json::from_str(&production_conf).expect("valid JSON");

    assert_eq!(
        production["build"]["frontendDist"],
        "../gtl-web/dist/public"
    );
    assert!(production["build"]["devUrl"].is_null());

    let dioxus_conf = fs::read_to_string(manifest_dir.join("../gtl-web/Dioxus.toml"))
        .expect("gtl-web/Dioxus.toml must exist");
    let dioxus: toml::Value = toml::from_str(&dioxus_conf).expect("valid Dioxus TOML");
    assert_eq!(dioxus["application"]["out_dir"].as_str(), Some("dist"));
    assert!(
        dioxus["application"].get("tailwind_input").is_none(),
        "the locked xtask owns release stylesheet generation"
    );
}

#[test]
fn production_flavor_has_a_local_only_wasm_and_ipc_csp() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let production_conf = fs::read_to_string(manifest_dir.join("tauri.production.conf.json"))
        .expect("tauri.production.conf.json must exist");
    let production: serde_json::Value = serde_json::from_str(&production_conf).expect("valid JSON");
    let csp = production["app"]["security"]["csp"]
        .as_str()
        .expect("production CSP must be a policy string");

    let directives = csp.split(';').map(str::trim).collect::<Vec<_>>();
    assert_eq!(
        directives,
        [
            "default-src 'self'",
            "base-uri 'self'",
            "connect-src 'self' ipc: http://ipc.localhost",
            "font-src 'self' data:",
            "form-action 'none'",
            "frame-src 'none'",
            "img-src 'self' data:",
            "object-src 'none'",
            "script-src 'self' 'unsafe-eval' 'wasm-unsafe-eval'",
            "style-src 'self' 'unsafe-inline'",
        ],
        "production CSP must remain the exact reviewed local policy"
    );
}
