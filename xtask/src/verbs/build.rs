//! Release build orchestration for the independent CLI and desktop artifacts.

use std::{ffi::OsStr, path::Path};

use anyhow::{Result, anyhow};

use super::{desktop_release, dioxus_web};
use crate::{cli::BuildTarget, process, project, task::Step};

const VIEWER_BUILD_ARGS: &[&str] = &[
    "build",
    "--release",
    "-p",
    "gtl-desktop",
    "--features",
    desktop_release::PRODUCTION_FEATURES,
];

/// Build the selected release artifact set. Every selected artifact is mandatory.
pub fn run(target: BuildTarget) -> Result<()> {
    let root = project::repository_root();
    let _lock = project::lock_web_assets(&root)?;
    for stage in build_stages(target) {
        match stage {
            BuildStage::ArtifactAssets => dioxus_web::build_artifact_assets_unlocked(&root)?,
            BuildStage::DioxusWeb => dioxus_web::build_release_unlocked(&root)?,
            BuildStage::Cli => build_cli(&root)?,
            BuildStage::Viewer => build_viewer(&root)?,
        }
    }
    Ok(())
}

fn build_cli(root: &Path) -> Result<()> {
    process::run_step(
        &Step::new(
            "cli-release-build",
            "cargo",
            ["build", "--release", "-p", "gtl-cli", "-p", "gtl-daemon"],
        )
        .with_current_directory(root),
    )
}

fn build_viewer(root: &Path) -> Result<()> {
    if std::env::consts::OS == "linux" {
        process::run_captured_with_env(
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
    desktop_release::run_cargo_unlocked("viewer-release-build", VIEWER_BUILD_ARGS, root)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuildStage {
    ArtifactAssets,
    DioxusWeb,
    Cli,
    Viewer,
}

fn build_stages(target: BuildTarget) -> &'static [BuildStage] {
    match target {
        BuildTarget::Cli => &[BuildStage::ArtifactAssets, BuildStage::Cli],
        BuildTarget::Viewer => &[BuildStage::DioxusWeb, BuildStage::Viewer],
        BuildTarget::Both => &[BuildStage::DioxusWeb, BuildStage::Cli, BuildStage::Viewer],
    }
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
                "gtl-desktop",
                "--features",
                "custom-protocol"
            ]
        );
    }

    #[test]
    fn viewer_build_stages_dioxus_transaction_before_cargo() {
        assert_eq!(
            build_stages(BuildTarget::Cli),
            [BuildStage::ArtifactAssets, BuildStage::Cli]
        );
        assert_eq!(
            build_stages(BuildTarget::Viewer),
            [BuildStage::DioxusWeb, BuildStage::Viewer]
        );
        assert_eq!(
            build_stages(BuildTarget::Both),
            [BuildStage::DioxusWeb, BuildStage::Cli, BuildStage::Viewer]
        );
    }
}
