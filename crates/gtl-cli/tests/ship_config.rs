#![cfg(test)]

//! Tripwire for the Windows MSVC linker flags required by shipped executables.
use std::{fs, path::Path};

#[test]
fn cargo_config_pins_windows_msvc_linker_flags() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cargo/config.toml"))
            .unwrap();
    let config: toml::Value = toml::from_str(&source).unwrap();
    let rustflags = config
        .get("target")
        .and_then(|target| target.get("x86_64-pc-windows-msvc"))
        .and_then(|target| target.get("rustflags"))
        .and_then(toml::Value::as_array)
        .unwrap()
        .iter()
        .map(toml::Value::as_str)
        .collect::<Option<Vec<_>>>()
        .unwrap();

    assert!(
        rustflags
            .windows(2)
            .any(|flags| flags == ["-C", "target-feature=+crt-static"]),
        "crt-static must be pinned for the Windows MSVC target (single-file exe)"
    );
    assert!(
        rustflags
            .windows(2)
            .any(|flags| flags == ["-C", "link-arg=/ignore:4099"]),
        "LNK4099 must be suppressed for cargo-xwin's CRT libraries"
    );
}
