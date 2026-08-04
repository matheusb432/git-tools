#![cfg(test)]

//! The walking-skeleton round trip: cli -> autostarted daemon -> store.
//!
//! Fixture helpers are copied from `crates/gtl-cli/tests/e2e.rs` rather than shared
//! across crates (this crate stays test-only, with no production `[lib]`).

use std::{
    fs::{OpenOptions, TryLockError},
    path::{Path, PathBuf},
    process::{self, Child, ExitStatus, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// Build both `git-tools` and `gtl-daemon` once per test binary — the daemon
/// lives in a sibling crate `assert_cmd` never builds on its own, so this
/// crate must build it explicitly for `cargo test -p gtl-e2e` to work standalone.
fn ensure_binaries_built() {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let mut command = std::process::Command::new(env!("CARGO"));
        command.args(["build", "-p", "gtl-cli", "-p", "gtl-daemon"]);
        if coverage_is_enabled() {
            command
                .arg("--target-dir")
                .arg(workspace_target_dir().join("e2e-llvm-cov-target"));
        }
        let status = command
            .status()
            .expect("cargo build -p gtl-cli -p gtl-daemon");
        assert!(status.success(), "building cli + daemon failed");
    });
}

fn coverage_is_enabled() -> bool {
    std::env::var_os("CARGO_LLVM_COV").is_some()
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
}

fn workspace_target_dir() -> PathBuf {
    workspace_root().join("target")
}

/// Absolute path of a workspace-built debug binary. Explicit because this
/// crate drives sibling-crate binaries: `CARGO_BIN_EXE_*` is never set for
/// them, and `assert_cmd`'s current-exe inference breaks under a shared Cargo
/// build directory. Coverage uses a separate nested target so instrumented
/// binaries cannot replace the ordinary `target/debug` artifacts.
fn workspace_bin(name: &str) -> PathBuf {
    ensure_binaries_built();
    let target_dir = if coverage_is_enabled() {
        workspace_target_dir().join("e2e-llvm-cov-target")
    } else {
        workspace_target_dir()
    };
    target_dir
        .join("debug")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

struct DaemonProcessGuard {
    cli_path: PathBuf,
    store_dir: PathBuf,
}

impl DaemonProcessGuard {
    fn new(store_dir: &Path) -> Self {
        Self {
            cli_path: workspace_bin("git-tools"),
            store_dir: store_dir.to_path_buf(),
        }
    }
}

impl Drop for DaemonProcessGuard {
    fn drop(&mut self) {
        match process::Command::new(&self.cli_path)
            .args(["daemon", "stop"])
            .env("GIT_TOOLS_DATA_DIR", &self.store_dir)
            .output()
        {
            Ok(output) if output.status.success() => {}
            Ok(output) => eprintln!(
                "daemon cleanup failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(error) => eprintln!("daemon cleanup failed: {error}"),
        }
    }
}

struct DirectDaemonGuard {
    child: Child,
}

impl DirectDaemonGuard {
    fn spawn(store_root: &Path) -> Self {
        Self::spawn_command(store_root, None)
    }

    fn spawn_in(store_root: &Path, working_directory: &Path) -> Self {
        Self::spawn_command(store_root, Some(working_directory))
    }

    fn spawn_command(store_root: &Path, working_directory: Option<&Path>) -> Self {
        let mut command = process::Command::new(workspace_bin("gtl-daemon"));
        if let Some(working_directory) = working_directory {
            command.current_dir(working_directory);
        }
        let child = command
            .env("GIT_TOOLS_DATA_DIR", store_root)
            .env_remove("GIT_TOOLS_DAEMON_PORT")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn gtl-daemon");
        Self { child }
    }

    fn id(&self) -> u32 {
        self.child.id()
    }

    fn try_wait(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().expect("poll gtl-daemon")
    }

    fn kill_and_wait(&mut self) {
        self.child.kill().expect("kill gtl-daemon test child");
        self.child.wait().expect("wait for killed gtl-daemon");
    }
}

impl Drop for DirectDaemonGuard {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

struct UnhealthyLockOwnerGuard {
    child: Child,
}

impl UnhealthyLockOwnerGuard {
    fn spawn(store_root: &Path) -> Self {
        let ready_path = store_root.join("lock-owner.ready");
        let child =
            process::Command::new(std::env::current_exe().expect("resolve e2e test binary"))
                .args([
                    "--ignored",
                    "--exact",
                    "daemon_lock_owner_process",
                    "--nocapture",
                ])
                .env("GIT_TOOLS_TEST_DAEMON_LOCK_OWNER", store_root)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn unhealthy lock owner");
        let owner = Self { child };
        assert!(
            wait_for(Duration::from_secs(5), || ready_path.exists()),
            "unhealthy lock owner must acquire daemon.lock"
        );
        owner
    }

    fn id(&self) -> u32 {
        self.child.id()
    }

    fn try_wait(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().expect("poll unhealthy lock owner")
    }
}

impl Drop for UnhealthyLockOwnerGuard {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

/// A throwaway git repo with one unpushed commit, and an isolated store dir.
struct Fixture {
    _tmp: TempDir,
    _daemon: DaemonProcessGuard,
    _store: TempDir,
    repo_path: PathBuf,
    store_dir: PathBuf,
}

impl Fixture {
    /// Initializes a repo (branch `main`, deterministic identity), pushes a
    /// base commit to a bare `origin`, then leaves one commit unpushed.
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let repo_path = tmp.path().join("repo");
        std::fs::create_dir_all(&repo_path).unwrap();
        let store_dir = store.path().to_path_buf();
        let daemon = DaemonProcessGuard::new(&store_dir);

        let this = Self {
            _tmp: tmp,
            _daemon: daemon,
            _store: store,
            repo_path,
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
        let out = process::Command::new("git")
            .arg("-C")
            .arg(&self.repo_path)
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
        let path = self.repo_path.join(file);
        std::fs::write(&path, contents).unwrap();
        self.git(&["add", "-A"]);
        let out = process::Command::new("git")
            .arg("-C")
            .arg(&self.repo_path)
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
        let remote = self.repo_path.parent().unwrap().join("origin.git");
        let out = process::Command::new("git")
            .args(["init", "--bare"])
            .arg(&remote)
            .output()
            .unwrap();
        assert!(out.status.success());
        self.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        self.git(&["push", "-u", "origin", "HEAD"]);
    }

    /// A `git-tools` invocation in the repo, with browser opening disabled and
    /// daemon data isolated in this fixture.
    fn run(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(workspace_bin("git-tools"));
        cmd.args(args)
            .current_dir(&self.repo_path)
            .env("GIT_TOOLS_DATA_DIR", &self.store_dir)
            // This suite pins the daemon/render round trip directly (autostart, sidecar
            // reuse); simulate headless so `diff`'s default degrades to that path
            // without needing `--raw` (Phase 5, Task 1.6).
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY");
        cmd
    }

    fn port_file_path(&self) -> PathBuf {
        self.store_dir.join("daemon.json")
    }

    /// Reads `<store>/daemon.json` and returns its `pid` field.
    fn daemon_pid(&self) -> u32 {
        daemon_pid_from(&self.store_dir)
    }
}

fn daemon_pid_from(store_root: &Path) -> u32 {
    let raw = std::fs::read_to_string(store_root.join("daemon.json"))
        .expect("daemon.json must exist after the daemon autostarts");
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    u32::try_from(json["pid"].as_u64().expect("daemon.json has a pid field")).unwrap()
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

    // 4. Exactly one artifact + sidecar landed in the repository-local store.
    let artifact = artifact_from_stdout(&stdout);
    assert!(artifact.exists(), "artifact must exist at {artifact:?}");
    let artifact_root = fixture.repo_path.join(".artifacts/gtl");
    assert!(
        artifact
            .parent()
            .is_some_and(|parent| parent == artifact_root),
        "artifact must be directly under the repository-local root: {artifact:?}"
    );
    assert_eq!(
        std::fs::read_to_string(artifact_root.join(".gitignore")).unwrap(),
        "*\n"
    );
    assert_eq!(
        fixture.git(&["status", "--short"]),
        "",
        "raw artifacts must not pollute Git status"
    );
    let html_files: Vec<PathBuf> = std::fs::read_dir(&artifact_root)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
        .collect();
    let json_files: Vec<PathBuf> = std::fs::read_dir(&artifact_root)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    assert_eq!(html_files.len(), 1, "expected exactly one artifact");
    assert_eq!(json_files.len(), 1, "expected exactly one sidecar");
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

fn init_tag_bump_repo(repository: &Path, base_tag: &str) {
    std::fs::create_dir_all(repository).unwrap();
    git_at(repository, &["init", "-q", "-b", "main"]);
    git_at(repository, &["config", "user.name", "E2E Bot"]);
    git_at(repository, &["config", "user.email", "e2e@example.invalid"]);
    std::fs::write(repository.join("release.txt"), "release\n").unwrap();
    git_at(repository, &["add", "release.txt"]);
    git_at(repository, &["commit", "-qm", "initial"]);
    git_at(
        repository,
        &["tag", "-a", base_tag, "-m", "initial release"],
    );
}

fn git_at(repository: &Path, arguments: &[&str]) -> String {
    let output = process::Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn tag_bump_uses_the_cli_repository_when_daemon_started_elsewhere() {
    let temporary = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let repository_a = temporary.path().join("repository-a");
    let repository_b = temporary.path().join("repository-b");
    init_tag_bump_repo(&repository_a, "v9.0.0");
    init_tag_bump_repo(&repository_b, "v1.2.3");

    let mut daemon = DirectDaemonGuard::spawn_in(store.path(), &repository_a);
    assert!(
        wait_for(Duration::from_secs(5), || {
            std::fs::read_to_string(store.path().join("daemon.json"))
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|json| json["pid"].as_u64())
                == Some(u64::from(daemon.id()))
        }),
        "daemon started from repository A must publish its discovery record"
    );

    let run_cli = |arguments: &[&str]| {
        let mut command = Command::new(workspace_bin("git-tools"));
        command
            .args(arguments)
            .current_dir(&repository_b)
            .env("GIT_TOOLS_DATA_DIR", store.path());
        command
    };

    let dry_run = run_cli(&["tag", "bump", "patch", "release B", "--dry"])
        .output()
        .unwrap();
    let dry_stdout = String::from_utf8(dry_run.stdout).unwrap();
    let dry_stderr = String::from_utf8(dry_run.stderr).unwrap();
    assert!(
        dry_run.status.success(),
        "dry-run failed\nstdout: {dry_stdout}\nstderr: {dry_stderr}"
    );
    assert!(
        dry_stdout.contains(&format!(
            "repository: repository-b ({})",
            repository_b.display()
        )),
        "stdout: {dry_stdout}"
    );
    assert!(
        dry_stdout.contains("base tag: v1.2.3"),
        "stdout: {dry_stdout}"
    );
    assert!(
        dry_stdout.contains("create: annotated tag v1.2.4"),
        "stdout: {dry_stdout}"
    );
    assert!(!dry_stdout.contains(&repository_a.display().to_string()));
    assert!(!dry_stdout.contains("v9.0.1"));

    let applied = run_cli(&["tag", "bump", "patch", "release B", "--yes"])
        .output()
        .unwrap();
    let applied_stdout = String::from_utf8(applied.stdout).unwrap();
    let applied_stderr = String::from_utf8(applied.stderr).unwrap();
    assert!(
        applied.status.success(),
        "bump failed\nstdout: {applied_stdout}\nstderr: {applied_stderr}"
    );
    assert!(
        applied_stdout.contains("created tag v1.2.4"),
        "stdout: {applied_stdout}"
    );
    assert_eq!(
        git_at(&repository_b, &["tag", "--list", "v1.2.4"]),
        "v1.2.4"
    );
    assert!(git_at(&repository_a, &["tag", "--list", "v1.2.4"]).is_empty());
    daemon.kill_and_wait();
}

#[test]
fn daemon_restart_replaces_a_healthy_process() {
    let fixture = Fixture::new();
    fixture.run(&["diff"]).assert().success();
    let pid_before = fixture.daemon_pid();

    fixture
        .run(&["daemon", "restart"])
        .assert()
        .success()
        .stdout(contains("gtl-daemon restarted"));

    let pid_after = fixture.daemon_pid();
    assert_ne!(pid_after, pid_before);
    fixture
        .run(&["daemon", "status"])
        .assert()
        .success()
        .stdout(contains(format!("pid {pid_after}")));
}

#[test]
fn ordinary_command_recovers_after_the_daemon_crashes() {
    let fixture = Fixture::new();
    let mut daemon = DirectDaemonGuard::spawn(&fixture.store_dir);
    let pid_before = daemon.id();
    assert!(
        wait_for(Duration::from_secs(5), || {
            fixture.port_file_path().exists() && fixture.daemon_pid() == pid_before
        }),
        "direct daemon must publish its discovery record"
    );
    daemon.kill_and_wait();

    fixture.run(&["diff"]).assert().success();

    let pid_after = fixture.daemon_pid();
    assert_ne!(pid_after, pid_before);
    fixture
        .run(&["daemon", "status"])
        .assert()
        .success()
        .stdout(contains(format!("pid {pid_after}")));
}

#[test]
fn unhealthy_lock_owner_blocks_restart_without_being_killed() {
    let store = tempfile::tempdir().unwrap();
    let mut owner = UnhealthyLockOwnerGuard::spawn(store.path());
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    std::fs::write(
        store.path().join("daemon.json"),
        format!(r#"{{"port":{port},"pid":{}}}"#, owner.id()),
    )
    .unwrap();

    let status_output = process::Command::new(workspace_bin("git-tools"))
        .args(["daemon", "status"])
        .env("GIT_TOOLS_DATA_DIR", store.path())
        .output()
        .expect("status with unhealthy lock owner");
    assert_eq!(status_output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&status_output.stderr)
            .contains("daemon lock is owned but no healthy daemon responded"),
        "stderr: {}",
        String::from_utf8_lossy(&status_output.stderr)
    );
    assert!(
        owner.try_wait().is_none(),
        "status must not kill the lock owner"
    );

    let started_at = Instant::now();
    let output = process::Command::new(workspace_bin("git-tools"))
        .args(["daemon", "restart"])
        .env("GIT_TOOLS_DATA_DIR", store.path())
        .output()
        .expect("restart with unhealthy lock owner");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("daemon lock is owned but no healthy daemon responded"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(started_at.elapsed() < Duration::from_secs(7));
    assert!(
        owner.try_wait().is_none(),
        "the CLI must not kill a PID from daemon.json"
    );
    assert_eq!(daemon_pid_from(store.path()), owner.id());
}

#[test]
#[ignore = "subprocess helper"]
fn daemon_lock_owner_process() {
    let Some(store_root) = std::env::var_os("GIT_TOOLS_TEST_DAEMON_LOCK_OWNER") else {
        return;
    };
    let store_root = PathBuf::from(store_root);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store_root.join("daemon.lock"))
        .unwrap();
    lock.lock().unwrap();
    std::fs::write(store_root.join("lock-owner.ready"), b"ready").unwrap();
    std::thread::sleep(Duration::from_secs(30));
}

#[test]
fn simultaneous_daemon_starts_leave_one_healthy_owner() {
    let store = tempfile::tempdir().unwrap();
    let mut first_daemon = DirectDaemonGuard::spawn(store.path());
    let mut second_daemon = DirectDaemonGuard::spawn(store.path());

    assert!(
        wait_for(Duration::from_secs(5), || {
            first_daemon.try_wait().is_some() || second_daemon.try_wait().is_some()
        }),
        "one competing daemon must yield within the startup budget"
    );

    let first_status = first_daemon.try_wait();
    let second_status = second_daemon.try_wait();
    let (loser_status, survivor_pid) = match (first_status, second_status) {
        (Some(status), None) => (status, second_daemon.id()),
        (None, Some(status)) => (status, first_daemon.id()),
        pair => panic!("exactly one daemon must remain running, got {pair:?}"),
    };
    assert!(loser_status.success(), "the lock loser should exit cleanly");

    assert!(
        wait_for(Duration::from_secs(5), || {
            std::fs::read_to_string(store.path().join("daemon.json"))
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|json| json["pid"].as_u64())
                == Some(u64::from(survivor_pid))
        }),
        "the surviving daemon must publish its own discovery record"
    );

    let status_output = process::Command::new(workspace_bin("git-tools"))
        .args(["daemon", "status"])
        .env("GIT_TOOLS_DATA_DIR", store.path())
        .output()
        .expect("query daemon status");
    assert!(
        status_output.status.success(),
        "daemon status failed: {}",
        String::from_utf8_lossy(&status_output.stderr)
    );
    let status_stdout = String::from_utf8(status_output.stdout).unwrap();
    assert!(status_stdout.contains("gtl-daemon running"));
    assert!(status_stdout.contains(&format!("pid {survivor_pid}")));

    let stop_output = process::Command::new(workspace_bin("git-tools"))
        .args(["daemon", "stop"])
        .env("GIT_TOOLS_DATA_DIR", store.path())
        .output()
        .expect("stop surviving daemon");
    assert!(
        stop_output.status.success(),
        "daemon stop failed: {}",
        String::from_utf8_lossy(&stop_output.stderr)
    );
    assert!(
        wait_for(Duration::from_secs(2), || {
            first_daemon.try_wait().is_some() && second_daemon.try_wait().is_some()
        }),
        "both child handles must observe exit"
    );
}

#[test]
fn daemon_stop_quiesces_an_elected_startup() {
    let store = tempfile::tempdir().unwrap();
    let ownership_probe = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.path().join("daemon.lock"))
        .unwrap();
    ownership_probe.lock_shared().unwrap();
    let mut daemon = DirectDaemonGuard::spawn(store.path());
    let startup_path = store.path().join("daemon.start.lock");
    assert!(
        wait_for(Duration::from_secs(2), || {
            let Ok(startup_probe) = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&startup_path)
            else {
                return false;
            };
            match startup_probe.try_lock() {
                Err(TryLockError::WouldBlock) => true,
                Ok(()) => {
                    startup_probe.unlock().unwrap();
                    false
                }
                Err(TryLockError::Error(error)) => {
                    panic!("inspect startup election: {error}")
                }
            }
        }),
        "daemon must own the startup election before stop begins"
    );

    let stop_output = process::Command::new(workspace_bin("git-tools"))
        .args(["daemon", "stop"])
        .env("GIT_TOOLS_DATA_DIR", store.path())
        .output()
        .expect("stop elected daemon startup");

    assert!(
        stop_output.status.success(),
        "daemon stop failed: {}",
        String::from_utf8_lossy(&stop_output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&stop_output.stdout).contains("gtl-daemon not running"),
        "stdout: {}",
        String::from_utf8_lossy(&stop_output.stdout)
    );
    drop(ownership_probe);
    assert!(
        wait_for(Duration::from_secs(2), || daemon.try_wait().is_some()),
        "the elected daemon must not start after stop returns"
    );

    let status_output = process::Command::new(workspace_bin("git-tools"))
        .args(["daemon", "status"])
        .env("GIT_TOOLS_DATA_DIR", store.path())
        .output()
        .expect("query daemon status after stop");
    assert!(status_output.status.success());
    assert!(String::from_utf8_lossy(&status_output.stdout).contains("gtl-daemon not running"));
}
