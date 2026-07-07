//! The walking-skeleton round trip: cli -> autostarted daemon -> store.
//!
//! Fixture helpers are copied from `crates/cli/tests/e2e.rs` rather than shared
//! across crates (this crate stays test-only, with no production `[lib]`).

use std::{
    path::{Path, PathBuf},
    process::Command as Git,
    sync::OnceLock,
    time::{Duration, Instant},
};

use assert_cmd::Command;
use infra::store::Sidecar;
use predicates::str::contains;
use tempfile::TempDir;

/// Build both `git-tools` and `gtl-daemon` once per test binary — the daemon
/// lives in a sibling crate `assert_cmd` never builds on its own, so this
/// crate must build it explicitly for `cargo test -p e2e` to work standalone.
fn ensure_binaries_built() {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "-p", "cli", "-p", "daemon"])
            .status()
            .expect("cargo build -p cli -p daemon");
        assert!(status.success(), "building cli + daemon failed");
    });
}

/// A throwaway git repo with one unpushed commit, and an isolated store dir.
struct Fixture {
    _tmp: TempDir,
    _store: TempDir,
    repo: PathBuf,
    store_dir: PathBuf,
}

impl Fixture {
    /// Initializes a repo (branch `main`, deterministic identity), pushes a
    /// base commit to a bare `origin`, then leaves one commit unpushed.
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let store_dir = store.path().to_path_buf();

        let this = Self {
            _tmp: tmp,
            _store: store,
            repo,
            store_dir,
        };
        this.git(&["init", "-b", "main"]);
        this.git(&["config", "user.name", "E2E Bot"]);
        this.git(&["config", "user.email", "e2e@example.invalid"]);
        this.git(&["config", "commit.gpgsign", "false"]);
        this.git(&["config", "core.autocrlf", "false"]);
        this.commit("a.txt", "base\n", "chore: base");
        this.add_upstream();
        this.commit("a.txt", "base\nlocal\n", "feat: local work");
        this
    }

    /// Runs `git -C <repo> <args>`, asserting success, and returns trimmed stdout.
    fn git(&self, args: &[&str]) -> String {
        let out = Git::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// Writes `file` then commits it with a fixed date; returns the new full SHA.
    fn commit(&self, file: &str, contents: &str, message: &str) -> String {
        let path = self.repo.join(file);
        std::fs::write(&path, contents).unwrap();
        self.git(&["add", "-A"]);
        let out = Git::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(["commit", "-m", message])
            .env("GIT_AUTHOR_DATE", "2026-01-01T12:00:00")
            .env("GIT_COMMITTER_DATE", "2026-01-01T12:00:00")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "commit failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        self.git(&["rev-parse", "HEAD"])
    }

    /// Adds a bare `origin` and pushes `main`, establishing an upstream (`@{u}`).
    fn add_upstream(&self) {
        let remote = self.repo.parent().unwrap().join("origin.git");
        let out = Git::new("git")
            .args(["init", "--bare"])
            .arg(&remote)
            .output()
            .unwrap();
        assert!(out.status.success());
        self.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        self.git(&["push", "-u", "origin", "HEAD"]);
    }

    /// A `git-tools` invocation in the repo, with browser-open disabled and the
    /// store pinned to this fixture's tempdir. The idle timeout is long enough
    /// that only this test's explicit `daemon stop` ends the daemon.
    fn run(&self, args: &[&str]) -> Command {
        ensure_binaries_built();
        let mut cmd = Command::cargo_bin("git-tools").unwrap();
        cmd.args(args)
            .current_dir(&self.repo)
            .env("GIT_TOOLS_NO_OPEN", "1")
            .env("GIT_TOOLS_DATA_DIR", &self.store_dir)
            .env("GIT_TOOLS_DAEMON_IDLE_SECS", "30");
        cmd
    }

    fn port_file_path(&self) -> PathBuf {
        self.store_dir.join("daemon.json")
    }

    /// Reads `<store>/daemon.json` and returns its `pid` field.
    fn daemon_pid(&self) -> u32 {
        let raw = std::fs::read_to_string(self.port_file_path())
            .expect("daemon.json must exist after the daemon autostarts");
        let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
        u32::try_from(json["pid"].as_u64().expect("daemon.json has a pid field")).unwrap()
    }
}

/// Polls `check` every 50ms until it returns `true` or `deadline` elapses.
fn wait_for(deadline: Duration, mut check: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if check() {
            return true;
        }
        if start.elapsed() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Extract the artifact path from a "wrote <path>" stdout line.
fn artifact_from_stdout(stdout: &str) -> PathBuf {
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("wrote ") {
            return PathBuf::from(path.trim());
        }
    }
    panic!("no 'wrote <path>' line found in stdout:\n{stdout}");
}

/// The single repo dir under `<store>/diffs/`, panicking if there isn't exactly one.
fn only_repo_diffs_dir(store_dir: &Path) -> PathBuf {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(store_dir.join("diffs"))
        .expect("diffs/ must exist after a render")
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .collect();
    assert_eq!(
        dirs.len(),
        1,
        "expected exactly one repo dir under diffs/, found {dirs:?}"
    );
    dirs.pop().unwrap()
}

#[test]
fn cli_autostarts_the_daemon_renders_and_stops() {
    // 1. Fixture repo with one unpushed commit; fresh tempdir data dir.
    let fixture = Fixture::new();

    // 2. `git-tools diff` autostarts the daemon and renders through it.
    let output = fixture.run(&["diff"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        output.status.success(),
        "diff failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("diff-preview:"), "stdout: {stdout}");
    assert!(stdout.contains("wrote "), "stdout: {stdout}");

    // 3. The daemon registered itself under the store's port file.
    assert!(
        fixture.port_file_path().exists(),
        "daemon.json must exist after autostart"
    );
    let first_pid = fixture.daemon_pid();
    fixture
        .run(&["daemon", "status"])
        .assert()
        .success()
        .stdout(contains("gtl-daemon running"))
        .stdout(contains(format!("pid {first_pid}")));

    // 4. Exactly one artifact + sidecar landed under the content-addressed store.
    let artifact = artifact_from_stdout(&stdout);
    assert!(artifact.exists(), "artifact must exist at {artifact:?}");
    let repo_dir = only_repo_diffs_dir(&fixture.store_dir);
    let html_files: Vec<PathBuf> = std::fs::read_dir(&repo_dir)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
        .collect();
    let json_files: Vec<PathBuf> = std::fs::read_dir(&repo_dir)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    assert_eq!(html_files.len(), 1, "expected exactly one artifact");
    assert_eq!(json_files.len(), 1, "expected exactly one sidecar");
    let sidecar: Sidecar = serde_json::from_str(&std::fs::read_to_string(&json_files[0]).unwrap())
        .expect("sidecar must deserialize via infra::store::Sidecar");
    assert!(!sidecar.base_sha.is_empty(), "sidecar.base_sha must be set");
    assert!(!sidecar.head_sha.is_empty(), "sidecar.head_sha must be set");
    assert_ne!(
        sidecar.base_sha, sidecar.head_sha,
        "an unpushed diff spans distinct base/head commits"
    );

    // 5. A second identical run hits the fast path: same artifact, no respawn.
    let output = fixture.run(&["diff"]).output().unwrap();
    let stdout2 = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "second diff failed: {stdout2}");
    assert!(stdout2.contains("reusing"), "stdout: {stdout2}");
    assert_eq!(
        fixture.daemon_pid(),
        first_pid,
        "the warm daemon must be reused, not respawned"
    );

    // 6. `daemon status` reports the still-running daemon.
    fixture
        .run(&["daemon", "status"])
        .assert()
        .success()
        .stdout(contains("gtl-daemon running"));

    // 7. `daemon stop` terminates it; the port file disappears within budget.
    fixture
        .run(&["daemon", "stop"])
        .assert()
        .success()
        .stdout(contains("gtl-daemon stopped"));
    assert!(
        wait_for(Duration::from_secs(2), || !fixture
            .port_file_path()
            .exists()),
        "daemon.json must be removed after `daemon stop`"
    );

    // 8. `daemon status` now reports nothing running.
    fixture
        .run(&["daemon", "status"])
        .assert()
        .success()
        .stdout(contains("gtl-daemon not running"));
}
