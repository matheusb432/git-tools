use std::path::PathBuf;

use anyhow::{Context as _, Result, ensure};
use assert_cmd::Command;
use predicates::str::contains;

mod common;

const REVIEW_PACKAGE: &str = "\
# Review package: 1111111..2222222

## Commits
2222222 docs(plan): archive rollout

## Diff
diff --git a/plans/rollout.md b/plans/archived/rollout.md
similarity index 99%
rename from plans/rollout.md
rename to plans/archived/rollout.md
index 1111111..2222222 100644
--- a/plans/rollout.md
+++ b/plans/archived/rollout.md
@@ -1,2 +1,2 @@
-status: executing
+status: done
 date: 2026-07-16
";

fn artifact_path(stdout: &[u8]) -> Result<PathBuf> {
    let stdout = String::from_utf8(stdout.to_vec())?;
    let url = stdout
        .lines()
        .find(|line| line.starts_with("file://"))
        .context("render must print an artifact URL")?;
    Ok(PathBuf::from(
        url.strip_prefix(if cfg!(windows) { "file:///" } else { "file://" })
            .context("artifact must use the platform's local file URL shape")?,
    ))
}

#[test]
fn patch_text_renders_standalone_artifacts_from_files_and_stdin() -> Result<()> {
    let viewer_stub = tempfile::tempdir()?;
    std::fs::write(
        viewer_stub
            .path()
            .join(format!("gtl-viewer{}", std::env::consts::EXE_SUFFIX)),
        "",
    )?;
    let inherited_path = std::env::var_os("PATH").context("missing executable path")?;
    let executable_path = std::env::join_paths(
        std::iter::once(viewer_stub.path().to_path_buf())
            .chain(std::env::split_paths(&inherited_path)),
    )?;
    // This file runs one test, so process-wide variables are set before the server or CLI start.
    unsafe {
        std::env::set_var("PATH", executable_path);
        std::env::remove_var("DISPLAY");
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("GIT_TOOLS_NO_OPEN");
    }
    let _server = common::ServerHarness::start(None, None)?;
    let data_root = PathBuf::from(std::env::var_os("GIT_TOOLS_DATA_DIR").context("data root")?);
    let inputs = tempfile::tempdir()?;
    let package = inputs.path().join("review-1111111..2222222.diff");
    std::fs::write(&package, REVIEW_PACKAGE)?;

    let output = Command::new(common::cli_binary())
        .current_dir(inputs.path())
        .args(["diff", "--raw", "--patch"])
        .arg(&package)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let artifact = artifact_path(&output)?;
    ensure!(
        artifact.starts_with(data_root.join("artifacts")),
        "text artifacts belong under the data root, not {}",
        artifact.display()
    );
    let html = std::fs::read_to_string(&artifact)?;
    ensure!(
        html.contains("plans/archived/rollout.md"),
        "artifact omits the renamed file"
    );
    ensure!(
        html.contains("review-1111111..2222222.diff"),
        "artifact omits the file label"
    );
    ensure!(
        !html.contains("Review package"),
        "artifact shows preamble text"
    );

    let output = Command::new(common::cli_binary())
        .args(["diff", "--raw", "--patch", "-", "-n", "piped review"])
        .write_stdin(REVIEW_PACKAGE)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let html = std::fs::read_to_string(artifact_path(&output)?)?;
    ensure!(
        html.contains("piped review"),
        "artifact omits the render name"
    );

    Command::new(common::cli_binary())
        .args(["diff", "--patch"])
        .arg(&package)
        .assert()
        .success()
        .stderr(contains("diff: viewer unavailable"))
        .stdout(contains("file://"));

    Command::new(common::cli_binary())
        .args(["diff", "--raw", "--patch", "-"])
        .write_stdin("no file sections here\n")
        .assert()
        .code(2)
        .stdout("")
        .stderr(contains("no `diff --git` file sections"));
    Ok(())
}
