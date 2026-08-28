#![cfg(test)]

//! Security posture checks for the server-backed Dioxus viewer.
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
fn development_flavor_uses_the_dioxus_server_with_an_independent_identity() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let release_conf = fs::read_to_string(manifest_dir.join("tauri.conf.json"))
        .expect("tauri.conf.json must exist");
    let development_conf = fs::read_to_string(manifest_dir.join("tauri.dev.conf.json"))
        .expect("tauri.dev.conf.json must exist");
    let release: serde_json::Value = serde_json::from_str(&release_conf).expect("valid JSON");
    let development: serde_json::Value =
        serde_json::from_str(&development_conf).expect("valid JSON");

    assert_eq!(release["identifier"], "dev.gittools.viewer");
    assert_eq!(release["app"]["withGlobalTauri"], false);
    assert_eq!(development["identifier"], "dev.gittools.viewer.dev");
    assert_ne!(development["identifier"], release["identifier"]);
    assert_eq!(development["build"]["devUrl"], "http://127.0.0.1:8080");
    assert_eq!(
        development["build"]["beforeDevCommand"]["script"],
        "cargo run --quiet -p xtask -- web-serve -- --addr 127.0.0.1 --port 8080 --open false --interactive false"
    );
    assert_eq!(development["build"]["beforeDevCommand"]["cwd"], "../..");
    assert_eq!(development["build"]["beforeDevCommand"]["wait"], false);
    assert!(development["build"]["frontendDist"].is_null());
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
fn macos_bundle_contains_the_cli_and_server_sidecars() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let macos_conf = fs::read_to_string(manifest_dir.join("tauri.macos.bundle.conf.json"))
        .expect("tauri.macos.bundle.conf.json must exist");
    let macos: serde_json::Value = serde_json::from_str(&macos_conf).expect("valid JSON");

    assert_eq!(macos["bundle"]["active"], true);
    assert_eq!(macos["bundle"]["targets"], serde_json::json!(["app"]));
    assert_eq!(
        macos["bundle"]["externalBin"],
        serde_json::json!(["binaries/git-tools", "binaries/gtl-server"])
    );
    assert_eq!(macos["bundle"]["macOS"]["signingIdentity"], "-");
}

#[test]
fn production_flavor_allows_wasm_without_a_network_endpoint() {
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
            "connect-src 'self'",
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
