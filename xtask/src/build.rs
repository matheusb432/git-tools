//! Release build orchestration for the independent CLI and desktop artifacts.

use std::ffi::OsStr;

use anyhow::{Result, anyhow};

use crate::{cli::BuildTarget, frontend, proc};

const VIEWER_BUILD_ARGS: &[&str] = &[
    "build",
    "--release",
    "-p",
    "desktop",
    "--features",
    "custom-protocol",
];

/// Build the selected release artifact set. Every selected artifact is mandatory.
pub fn run(target: BuildTarget) -> Result<()> {
    match target {
        BuildTarget::Cli => build_cli(),
        BuildTarget::Viewer => build_viewer(),
        BuildTarget::Both => {
            build_cli()?;
            build_viewer()
        }
    }
}

fn build_cli() -> Result<()> {
    frontend::build()?;
    proc::run(
        "cli-release-build",
        "cargo",
        &["build", "--release", "-p", "cli", "-p", "daemon"],
    )
}

fn build_viewer() -> Result<()> {
    if std::env::consts::OS == "linux" {
        proc::run_captured_with_env(
            "viewer-webkit-headers",
            None,
            OsStr::new("pkg-config"),
            &["--exists", "webkit2gtk-4.1"],
            &[],
        )
        .map_err(|error| {
            anyhow!(
                "webkit2gtk-4.1 development headers are required for `just desktop build`: {error:#}"
            )
        })?;
    }
    proc::run("viewer-release-build", "cargo", VIEWER_BUILD_ARGS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_build_is_release_custom_protocol_and_cannot_soft_skip() {
        assert_eq!(
            VIEWER_BUILD_ARGS,
            [
                "build",
                "--release",
                "-p",
                "desktop",
                "--features",
                "custom-protocol"
            ]
        );
    }
}
