//! Cross-build the Windows CLI, server, and offline desktop viewer from Linux with cargo-xwin.

use std::path::Path;

use anyhow::{Context, Result, bail};

use super::{cargo_target_directory, desktop_release, dioxus_web, repository_root};
use crate::{process, task::Step};

/// The Windows cross-target. Production builds enable Tauri's custom protocol for embedded App
/// assets.
const WIN_TARGET: &str = "x86_64-pc-windows-msvc";

fn viewer_build_arguments(smoke: bool) -> Vec<&'static str> {
    let mut arguments = vec!["xwin", "build"];
    if !smoke {
        arguments.push("--release");
    }
    arguments.extend_from_slice(&[
        "-p",
        "gtl-desktop",
        "--features",
        desktop_release::PRODUCTION_FEATURES,
        "--target",
        WIN_TARGET,
    ]);
    arguments
}

fn release_artifact_path(target: &Path, executable: &str) -> std::path::PathBuf {
    target.join(WIN_TARGET).join("release").join(executable)
}

/// Cross-build the Windows shippables. Smoke mode uses the debug Rust profile and skips artifact
/// verification; both modes stage the complete offline frontend first.
pub fn run(smoke: bool, force: bool) -> Result<()> {
    dioxus_web::build_release().context("ship Dioxus Web release bundle failed")?;

    if !force {
        process::run_step(&Step::new("ship-tests", "just", ["test", "--all"]))?;
    }

    let profile: &[&str] = if smoke { &[] } else { &["--release"] };
    let mut cli_args = vec!["xwin", "build"];
    cli_args.extend_from_slice(profile);
    cli_args.extend_from_slice(&["-p", "gtl-cli", "--target", WIN_TARGET]);

    let mut server_args = vec!["xwin", "build"];
    server_args.extend_from_slice(profile);
    server_args.extend_from_slice(&["-p", "gtl-server", "--target", WIN_TARGET]);

    let viewer_args = viewer_build_arguments(smoke);
    let root = repository_root();

    process::run_step(
        &Step::new("cross-build-cli", "cargo", cli_args).with_current_directory(&root),
    )
    .context("cross-build the Windows CLI")?;
    process::run_step(
        &Step::new("cross-build-server", "cargo", server_args).with_current_directory(&root),
    )
    .context("cross-build the Windows server")?;
    desktop_release::run_cargo("cross-build-viewer", &viewer_args)
        .context("cross-build the Windows Dioxus viewer")?;

    if !smoke {
        verify_release_artifacts(&root)?;
    }

    Ok(())
}

fn verify_release_artifacts(root: &Path) -> Result<()> {
    let target =
        cargo_target_directory(root).context("resolve Windows release artifact directory")?;
    for executable in ["git-tools.exe", "gtl-server.exe", "gtl-viewer.exe"] {
        let path = release_artifact_path(&target, executable);
        let bytes = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
        if bytes == 0 {
            bail!("ship verify: {} is missing or empty", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_cross_build_uses_the_production_app_feature() {
        assert_eq!(
            viewer_build_arguments(false),
            [
                "xwin",
                "build",
                "--release",
                "-p",
                "gtl-desktop",
                "--features",
                "custom-protocol",
                "--target",
                WIN_TARGET,
            ]
        );
    }

    #[test]
    fn release_artifacts_resolve_from_a_non_default_cargo_target() {
        assert_eq!(
            release_artifact_path(Path::new("non-default-target"), "gtl-viewer.exe"),
            Path::new("non-default-target")
                .join(WIN_TARGET)
                .join("release/gtl-viewer.exe")
        );
    }
}
