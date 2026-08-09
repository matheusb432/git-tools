//! Cross-build the Windows CLI, daemon, and offline desktop viewer from Linux with cargo-xwin.

use std::path::Path;

use anyhow::{Context, Result, bail};

use super::{desktop_release, dioxus_web};
use crate::{
    process::{self, Status},
    project,
    task::Step,
    verb::Verb,
};

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

fn cross_build_step(label: &str, arguments: &[&str], root: &Path) -> Step {
    Step::new(label, "cargo", arguments.iter().copied()).with_current_directory(root)
}

/// Cross-build the Windows shippables. Smoke mode uses the debug Rust profile and skips artifact
/// verification; both modes stage the complete offline frontend first.
pub fn run(smoke: bool, force: bool) -> Result<()> {
    // 1. Stage frontend bundles before drift-sensitive repository tests.
    if let Err(error) = dioxus_web::build_release() {
        process::result_fail_step(Verb::SHIP, "dioxus-web");
        return Err(error).context("ship Dioxus Web release bundle failed");
    }

    // 2. Repository gate; force skips only this test preflight.
    if !force {
        process::run("ship-tests", "just", &["test", "--all"])?;
    }

    // 3. Cross-build all three packages.
    let profile: &[&str] = if smoke { &[] } else { &["--release"] };
    let mut cli_args = vec!["xwin", "build"];
    cli_args.extend_from_slice(profile);
    cli_args.extend_from_slice(&["-p", "gtl-cli", "--target", WIN_TARGET]);

    let mut daemon_args = vec!["xwin", "build"];
    daemon_args.extend_from_slice(profile);
    daemon_args.extend_from_slice(&["-p", "gtl-daemon", "--target", WIN_TARGET]);

    let viewer_args = viewer_build_arguments(smoke);
    let root = project::repository_root();

    let cross_build = process::run_step(&cross_build_step("cross-build-cli", &cli_args, &root))
        .context("cross-build the Windows CLI")
        .and_then(|()| {
            process::run_step(&cross_build_step("cross-build-daemon", &daemon_args, &root))
                .context("cross-build the Windows daemon")
        })
        .and_then(|()| {
            desktop_release::run_cargo("cross-build-viewer", &viewer_args)
                .context("cross-build the Windows Dioxus viewer")
        });
    if let Err(error) = cross_build {
        process::result_fail_step(Verb::SHIP, "cross-build");
        return Err(error);
    }

    // 4. Verify artifacts (release only).
    if !smoke {
        let target = project::cargo_target_directory(&root)
            .context("resolve Windows release artifact directory")?;
        for exe in ["git-tools.exe", "gtl-daemon.exe", "gtl-viewer.exe"] {
            let path = release_artifact_path(&target, exe);
            let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
            if bytes == 0 {
                process::result_fail_step(Verb::SHIP, "verify");
                bail!("ship verify: {} is missing or empty", path.display());
            }
        }
    }

    process::result(Verb::SHIP, Status::Pass);
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

    #[test]
    fn cross_build_steps_run_from_the_repository_root() {
        let root = Path::new("repository-root");
        let step = cross_build_step("cross-build-cli", &["xwin", "build", "-p", "gtl-cli"], root);

        assert_eq!(step.program(), "cargo");
        assert_eq!(step.current_directory(), Some(root));
        assert_eq!(step.arguments(), ["xwin", "build", "-p", "gtl-cli"]);
    }
}
