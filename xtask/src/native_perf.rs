//! `xtask native-perf [--evidence] [--smoke]` — build the native viewer, generate
//! deterministic local fixtures, and drive the real Tauri app through `tauri-driver`.

use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::proc;

const PERF_SCOPE: &str = "native-perf";
const WDIO_PACKAGE_DIR: &str = "e2e/native-perf";
const SUCCESS_DIR: &str = ".artifacts/e2e/success/native-perf";
const FAIL_DIR: &str = ".artifacts/e2e/fail/native-perf";

pub fn run(evidence: bool, smoke: bool) -> Result<()> {
    let repo_root = repo_root()?;
    let tauri_driver = find_tauri_driver()?;
    ensure_linux_runtime_support()?;
    ensure_no_running_viewer()?;
    let git_metadata = GitMetadata::detect(&repo_root);
    let evidence_paths = evidence
        .then(|| EvidencePaths::prepare(&repo_root))
        .transpose()?;

    let harness_result = (|| -> Result<()> {
        build_viewer_ui(&repo_root)?;
        let viewer_binary = build_viewer_binary(&repo_root)?;
        let data_dir =
            tempfile::tempdir().context("failed to create isolated GIT_TOOLS_DATA_DIR")?;
        let fixture_root =
            tempfile::tempdir().context("failed to create isolated native perf fixture root")?;
        let envs = command_env(
            &repo_root,
            &tauri_driver,
            &viewer_binary,
            data_dir.path(),
            fixture_root.path(),
            evidence_paths.as_ref(),
            git_metadata.as_ref(),
            evidence,
            smoke,
        );

        run_package_command(
            &repo_root,
            "native-perf-fixtures",
            &["run", "--cwd", WDIO_PACKAGE_DIR, "fixtures"],
            false,
            &envs,
        )?;

        run_package_command(
            &repo_root,
            "native-perf-wdio",
            &["run", "--cwd", WDIO_PACKAGE_DIR, "test"],
            true,
            &envs,
        )
    })();

    match finalize_with_evidence(harness_result, evidence_paths.as_ref()) {
        Ok(()) => {
            proc::result(PERF_SCOPE, "OK");
            Ok(())
        }
        Err(error) => {
            proc::result_fail_step(PERF_SCOPE, "wdio");
            Err(error)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EvidencePaths {
    success_dir: PathBuf,
    fail_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitMetadata {
    commit: String,
    branch: Option<String>,
    dirty: bool,
}

impl GitMetadata {
    fn detect(repo_root: &Path) -> Option<Self> {
        let commit = git_output(repo_root, &["rev-parse", "HEAD"])?;
        let branch =
            git_output(repo_root, &["branch", "--show-current"]).filter(|value| !value.is_empty());
        let dirty = git_output(repo_root, &["status", "--porcelain"])
            .is_some_and(|value| !value.is_empty());

        Some(Self {
            commit,
            branch,
            dirty,
        })
    }
}

impl EvidencePaths {
    fn prepare(repo_root: &Path) -> Result<Self> {
        let success_dir = repo_root.join(SUCCESS_DIR);
        let fail_dir = repo_root.join(FAIL_DIR);

        for root in [&success_dir, &fail_dir] {
            if let Some(parent) = root.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "failed to create evidence parent directory {}",
                        parent.display()
                    )
                })?;
            }
        }

        for stale in [&success_dir, &fail_dir] {
            if stale.exists() {
                fs::remove_dir_all(stale).with_context(|| {
                    format!("failed to prune stale evidence at {}", stale.display())
                })?;
            }
        }

        fs::create_dir_all(&success_dir).with_context(|| {
            format!(
                "failed to create native perf success evidence dir {}",
                success_dir.display()
            )
        })?;

        Ok(Self {
            success_dir,
            fail_dir,
        })
    }

    fn finalize(&self, success: bool) -> Result<()> {
        if success || !self.success_dir.exists() {
            return Ok(());
        }

        if let Some(parent) = self.fail_dir.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create failure evidence parent {}",
                    parent.display()
                )
            })?;
        }

        if self.fail_dir.exists() {
            fs::remove_dir_all(&self.fail_dir).with_context(|| {
                format!(
                    "failed to replace stale failure evidence at {}",
                    self.fail_dir.display()
                )
            })?;
        }

        fs::rename(&self.success_dir, &self.fail_dir).with_context(|| {
            format!(
                "failed to move native perf evidence from {} to {}",
                self.success_dir.display(),
                self.fail_dir.display()
            )
        })?;
        Ok(())
    }
}

fn finalize_with_evidence(
    result: Result<()>,
    evidence_paths: Option<&EvidencePaths>,
) -> Result<()> {
    if let Some(paths) = evidence_paths {
        paths.finalize(result.is_ok())?;
    }
    result
}

fn command_env(
    repo_root: &Path,
    tauri_driver: &Path,
    viewer_binary: &Path,
    data_dir: &Path,
    fixture_root: &Path,
    evidence_paths: Option<&EvidencePaths>,
    git_metadata: Option<&GitMetadata>,
    evidence: bool,
    smoke: bool,
) -> BTreeMap<&'static str, String> {
    let mut envs = BTreeMap::from([
        (
            "GIT_TOOLS_DATA_DIR",
            data_dir.to_string_lossy().into_owned(),
        ),
        (
            "GTL_NATIVE_PERF_FIXTURE_ROOT",
            fixture_root.to_string_lossy().into_owned(),
        ),
        (
            "GTL_NATIVE_PERF_REPO_ROOT",
            repo_root.to_string_lossy().into_owned(),
        ),
        (
            "GTL_NATIVE_PERF_TAURI_DRIVER",
            tauri_driver.to_string_lossy().into_owned(),
        ),
        (
            "GTL_NATIVE_PERF_APP_BINARY",
            viewer_binary.to_string_lossy().into_owned(),
        ),
    ]);

    if let Some(paths) = evidence_paths {
        envs.insert(
            "GTL_NATIVE_PERF_ARTIFACT_DIR",
            paths.success_dir.to_string_lossy().into_owned(),
        );
    }
    if let Some(metadata) = git_metadata {
        envs.insert("GTL_NATIVE_PERF_GIT_COMMIT", metadata.commit.clone());
        if let Some(branch) = &metadata.branch {
            envs.insert("GTL_NATIVE_PERF_GIT_BRANCH", branch.clone());
        }
        envs.insert("GTL_NATIVE_PERF_GIT_DIRTY", metadata.dirty.to_string());
    }
    if evidence {
        envs.insert("GTL_NATIVE_PERF_EVIDENCE", "1".to_string());
    }
    if smoke {
        envs.insert("GTL_NATIVE_PERF_SMOKE", "1".to_string());
    }

    envs
}

fn repo_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .context("xtask/ must have the workspace root as its parent")
}

fn run_package_command(
    repo_root: &Path,
    label: &str,
    args: &[&str],
    needs_display: bool,
    envs: &BTreeMap<&'static str, String>,
) -> Result<()> {
    let mut command = if needs_display && cfg!(target_os = "linux") && !has_display() {
        let xvfb_run = which::which("xvfb-run").context(
            "Linux WebKit/Tauri runtime support is unavailable: no DISPLAY/WAYLAND_DISPLAY is set and `xvfb-run` is not installed.",
        )?;
        let mut wrapped = Command::new(xvfb_run);
        wrapped.args(["-a", "--server-args=-screen 0 1920x1080x24", "bun"]);
        wrapped.args(args);
        wrapped
    } else {
        let mut bun = Command::new("bun");
        bun.args(args);
        bun
    };
    command.current_dir(repo_root);
    command.envs(envs.iter().map(|(key, value)| (*key, value.as_str())));

    let status = command
        .status()
        .with_context(|| format!("failed to start `{label}` via bun"))?;
    if !status.success() {
        bail!("{label} failed (exit {})", status.code().unwrap_or(-1));
    }
    Ok(())
}

fn build_viewer_binary(repo_root: &Path) -> Result<PathBuf> {
    let status = Command::new("cargo")
        .current_dir(repo_root)
        .args([
            "build",
            "--release",
            "-p",
            "desktop",
            "--features",
            "custom-protocol",
        ])
        .status()
        .context("failed to start `cargo build` for gtl-viewer")?;
    if !status.success() {
        bail!(
            "cargo build for gtl-viewer failed (exit {})",
            status.code().unwrap_or(-1)
        );
    }

    let binary = repo_root
        .join("target")
        .join("release")
        .join(format!("gtl-viewer{}", env::consts::EXE_SUFFIX));
    if !binary.is_file() {
        bail!("expected viewer binary at {}", binary.display());
    }
    Ok(binary)
}

fn git_output(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
}

fn build_viewer_ui(repo_root: &Path) -> Result<()> {
    let status = Command::new("bunx")
        .current_dir(repo_root)
        .env("NODE_ENV", "production")
        .args([
            "vite",
            "build",
            "--config",
            "frontend/viewer/vite.config.mjs",
        ])
        .status()
        .context("failed to start `bunx vite build` for the viewer UI")?;
    if !status.success() {
        bail!(
            "viewer UI build failed (exit {})",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

fn ensure_linux_runtime_support() -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Ok(());
    }

    let webkit_pkg = Command::new("pkg-config")
        .args(["--exists", "webkit2gtk-4.1"])
        .status()
        .context("failed to probe `webkit2gtk-4.1` with pkg-config")?;
    if !webkit_pkg.success() {
        bail!(
            "Linux WebKit/Tauri runtime support is unavailable: `webkit2gtk-4.1` headers/runtime were not detected via pkg-config."
        );
    }

    if which::which("WebKitWebDriver").is_err() {
        bail!(
            "Linux WebKit/Tauri runtime support is unavailable: `WebKitWebDriver` is not installed or not on PATH."
        );
    }

    Ok(())
}

fn has_display() -> bool {
    has_display_vars(
        env::var_os("DISPLAY").as_deref(),
        env::var_os("WAYLAND_DISPLAY").as_deref(),
    )
}

fn has_display_vars(
    display: Option<&std::ffi::OsStr>,
    wayland_display: Option<&std::ffi::OsStr>,
) -> bool {
    display.is_some() || wayland_display.is_some()
}

fn find_tauri_driver() -> Result<PathBuf> {
    if let Ok(path) = env::var("TAURI_DRIVER") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Ok(candidate);
        }
        bail!(
            "TAURI_DRIVER points to {}, but that file does not exist",
            candidate.display()
        );
    }

    if let Ok(path) = which::which("tauri-driver") {
        return Ok(path);
    }

    if let Some(home) = env::var_os("HOME") {
        let cargo_bin = Path::new(&home)
            .join(".cargo")
            .join("bin")
            .join(format!("tauri-driver{}", env::consts::EXE_SUFFIX));
        if cargo_bin.is_file() {
            return Ok(cargo_bin);
        }
        bail!(
            "tauri-driver was not found on PATH. Expected it at {} or install it with `cargo install tauri-driver`.",
            cargo_bin.display()
        );
    }

    bail!(
        "tauri-driver was not found on PATH, and HOME is unavailable for the ~/.cargo/bin fallback"
    )
}

fn ensure_no_running_viewer() -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Ok(());
    }

    let output = Command::new("pgrep")
        .args(["-af", "gtl-viewer"])
        .output()
        .context("failed to probe for a running gtl-viewer instance with pgrep")?;

    if output.status.code() == Some(1) {
        return Ok(());
    }
    if !output.status.success() {
        bail!(
            "pgrep -af gtl-viewer failed (exit {})",
            output.status.code().unwrap_or(-1)
        );
    }

    let matches = String::from_utf8(output.stdout)
        .context("pgrep produced non-UTF-8 output while probing gtl-viewer")?;
    let running = matches
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if running.is_empty() {
        return Ok(());
    }

    bail!(
        "A gtl-viewer instance is already running, and the native perf harness must start its own isolated viewer session. Close the existing viewer first (single-instance mode otherwise steals the webdriver launch):\n{}",
        running.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_perf_env_sets_requested_mode_flags() {
        let repo_root = Path::new("/repo");
        let envs = command_env(
            repo_root,
            Path::new("/tooling/tauri-driver"),
            Path::new("/repo/target/release/gtl-viewer"),
            Path::new("/tmp/data"),
            Path::new("/tmp/fixtures"),
            Some(&EvidencePaths {
                success_dir: repo_root.join(SUCCESS_DIR),
                fail_dir: repo_root.join(FAIL_DIR),
            }),
            None,
            true,
            true,
        );

        assert_eq!(envs.get("GTL_NATIVE_PERF_EVIDENCE"), Some(&"1".to_string()));
        assert_eq!(envs.get("GTL_NATIVE_PERF_SMOKE"), Some(&"1".to_string()));
        assert_eq!(
            envs.get("GTL_NATIVE_PERF_ARTIFACT_DIR"),
            Some(&repo_root.join(SUCCESS_DIR).to_string_lossy().into_owned())
        );
    }

    #[test]
    fn native_perf_prepare_prunes_stale_failure_dir() {
        let temp = tempfile::TempDir::new().unwrap();
        let stale = temp.path().join(FAIL_DIR);
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join("old.json"), "{}").unwrap();

        let paths = EvidencePaths::prepare(temp.path()).unwrap();

        assert!(paths.success_dir.is_dir());
        assert!(!paths.fail_dir.exists());
    }

    #[test]
    fn native_perf_finalize_moves_failed_evidence_into_fail_dir() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = EvidencePaths::prepare(temp.path()).unwrap();
        fs::write(paths.success_dir.join("timings.json"), "{}").unwrap();

        paths.finalize(false).unwrap();

        assert!(!paths.success_dir.exists());
        assert!(paths.fail_dir.join("timings.json").is_file());
    }

    #[test]
    fn native_perf_detects_display_when_present() {
        assert!(has_display_vars(Some(std::ffi::OsStr::new(":99")), None));
        assert!(has_display_vars(
            None,
            Some(std::ffi::OsStr::new("wayland-0"))
        ));
        assert!(!has_display_vars(None, None));
    }

    #[test]
    fn native_perf_finalize_with_evidence_moves_prepared_success_dir_on_early_failure() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = EvidencePaths::prepare(temp.path()).unwrap();
        fs::write(paths.success_dir.join("timings.json"), "{}").unwrap();

        let result = finalize_with_evidence(Err(anyhow::anyhow!("boom")), Some(&paths));

        assert!(result.is_err());
        assert!(!paths.success_dir.exists());
        assert!(paths.fail_dir.join("timings.json").is_file());
    }

    #[test]
    fn native_perf_finalize_with_evidence_keeps_success_dir_for_passing_runs() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = EvidencePaths::prepare(temp.path()).unwrap();
        fs::write(paths.success_dir.join("timings.json"), "{}").unwrap();

        finalize_with_evidence(Ok(()), Some(&paths)).unwrap();

        assert!(paths.success_dir.join("timings.json").is_file());
        assert!(!paths.fail_dir.exists());
    }

    #[test]
    fn native_perf_command_env_leaves_driver_port_selection_to_wdio() {
        let repo_root = Path::new("/repo");
        let envs = command_env(
            repo_root,
            Path::new("/tooling/tauri-driver"),
            Path::new("/repo/target/release/gtl-viewer"),
            Path::new("/tmp/data"),
            Path::new("/tmp/fixtures"),
            None,
            None,
            false,
            false,
        );

        assert!(!envs.contains_key("GTL_NATIVE_PERF_TAURI_DRIVER_PORT"));
        assert!(!envs.contains_key("GTL_NATIVE_PERF_NATIVE_DRIVER_PORT"));
    }

    #[test]
    fn native_perf_command_env_includes_source_revision_identity() {
        let repo_root = Path::new("/repo");
        let envs = command_env(
            repo_root,
            Path::new("/tooling/tauri-driver"),
            Path::new("/repo/target/release/gtl-viewer"),
            Path::new("/tmp/data"),
            Path::new("/tmp/fixtures"),
            None,
            Some(&GitMetadata {
                commit: "abc123".to_string(),
                branch: Some("main".to_string()),
                dirty: true,
            }),
            false,
            false,
        );

        assert_eq!(
            envs.get("GTL_NATIVE_PERF_GIT_COMMIT"),
            Some(&"abc123".to_string())
        );
        assert_eq!(
            envs.get("GTL_NATIVE_PERF_GIT_BRANCH"),
            Some(&"main".to_string())
        );
        assert_eq!(
            envs.get("GTL_NATIVE_PERF_GIT_DIRTY"),
            Some(&"true".to_string())
        );
    }
}
