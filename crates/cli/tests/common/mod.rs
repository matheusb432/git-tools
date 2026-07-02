//! Shared e2e helpers. `gtl diff` now renders through a spawned `gtl-daemon`, so
//! the client's sibling resolution must find that binary under `target/<profile>/`
//! even when a diff test binary is run alone (`cargo test -p cli`).

use std::sync::OnceLock;

/// Build `gtl-daemon` once per test binary. Idempotent via a `OnceLock`, so
/// calling it from every diff test costs one `cargo build` and no more.
pub fn ensure_daemon_built() {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "-p", "daemon", "--bin", "gtl-daemon"])
            .status()
            .expect("cargo build -p daemon");
        assert!(status.success(), "building gtl-daemon failed");
    });
}
