//! Headless viewer-render benchmark entrypoint.

use anyhow::Result;

use crate::proc;

/// Run the pure renderer benchmark with host display variables removed.
pub fn run() -> Result<()> {
    proc::run_with_removed_env(
        "desktop-render-benchmark",
        "cargo",
        &[
            "bench",
            "-p",
            "desktop",
            "--bench",
            "viewer_render",
            "--features",
            "benchmark-support",
        ],
        &["DISPLAY", "WAYLAND_DISPLAY"],
    )
}
