//! Shared e2e helpers. `gtl diff` now renders through a spawned `gtl-daemon`, so
//! the client's sibling resolution must find that binary under `target/<profile>/`
//! even when a diff test binary is run alone (`cargo test -p cli`).

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::OnceLock,
};

/// Stops the daemon associated with one isolated test store when the fixture drops.
pub struct DaemonProcessGuard {
    cli_path: PathBuf,
    store_dir: PathBuf,
}

impl DaemonProcessGuard {
    /// Creates a cleanup guard for `store_dir`.
    pub fn new(store_dir: &Path) -> Self {
        Self {
            cli_path: assert_cmd::cargo::cargo_bin("git-tools"),
            store_dir: store_dir.to_path_buf(),
        }
    }
}

impl Drop for DaemonProcessGuard {
    fn drop(&mut self) {
        let mut command = std::process::Command::new(&self.cli_path);
        let _ = command
            .args(["daemon", "stop"])
            .env("GIT_TOOLS_DATA_DIR", &self.store_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output();
    }
}

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
