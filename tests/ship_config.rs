//! Tripwire: the Windows MSVC target must statically link the C runtime so each
//! shipped exe is a single self-contained file that runs on a clean Win11 box with
//! no Visual C++ Redistributable. Guards `.cargo/config.toml` against a silent
//! regression (GTL-0016 Phase 3). Plain-text checks, no TOML dep — mirrors
//! `gtl-platform`'s `cfg_guard` test.
use std::{fs, path::Path};

#[test]
fn cargo_config_pins_crt_static_for_windows_msvc() {
    let cfg = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(".cargo/config.toml"))
        .expect(".cargo/config.toml must exist (crt-static for the win-msvc ship)");
    assert!(
        cfg.contains("x86_64-pc-windows-msvc"),
        "config must target x86_64-pc-windows-msvc"
    );
    assert!(
        cfg.contains("target-feature=+crt-static"),
        "crt-static must be pinned for the Windows MSVC target (single-file exe)"
    );
}
