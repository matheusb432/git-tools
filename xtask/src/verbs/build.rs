//! Release build orchestration for the CLI and desktop viewer.

use std::path::Path;

use anyhow::{Result, anyhow};

use super::{desktop_release, dioxus_web, lock_web_assets, repository_root};
use crate::{cli::BuildTarget, process, task::Step};

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
    let root = repository_root();
    let _lock = matches!(target, BuildTarget::Viewer | BuildTarget::Both)
        .then(|| lock_web_assets(&root))
        .transpose()?;
    if matches!(target, BuildTarget::Viewer | BuildTarget::Both) {
        dioxus_web::build_release_unlocked(&root)?;
    }
    if matches!(target, BuildTarget::Cli | BuildTarget::Both) {
        build_cli(&root)?;
    }
    if matches!(target, BuildTarget::Viewer | BuildTarget::Both) {
        build_viewer(&root)?;
    }
    Ok(())
}

fn build_cli(root: &Path) -> Result<()> {
    process::run_step(
        &Step::new(
            "cli-release-build",
            "cargo",
            ["build", "--release", "-p", "gtl-cli", "-p", "gtl-server"],
        )
        .with_current_directory(root),
    )
}

fn build_viewer(root: &Path) -> Result<()> {
    if std::env::consts::OS == "linux" {
        process::run_step(&Step::new(
            "viewer-webkit-headers",
            "pkg-config",
            ["--exists", "webkit2gtk-4.1"],
        ))
        .map_err(|error| {
            anyhow!(
                "webkit2gtk-4.1 development headers are required for `just desktop build`: {error:#}"
            )
        })?;
    }
    desktop_release::run_cargo_unlocked("viewer-release-build", VIEWER_BUILD_ARGS, root)
}
