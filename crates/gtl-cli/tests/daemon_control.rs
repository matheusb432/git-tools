use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    process::{Child, ExitStatus, Output, Stdio},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};

struct UnhealthyLockOwnerGuard {
    child: Child,
}

impl UnhealthyLockOwnerGuard {
    fn spawn(store_root: &Path) -> Result<Self> {
        let ready_path = store_root.join("lock-owner.ready");
        let test_binary = std::env::current_exe().context("resolve test binary")?;
        let child = std::process::Command::new(test_binary)
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
            .context("spawn unhealthy lock owner")?;
        let owner = Self { child };
        assert!(
            wait_for(Duration::from_secs(5), || ready_path.exists()),
            "unhealthy lock owner must acquire daemon.lock"
        );
        Ok(owner)
    }

    fn id(&self) -> u32 {
        self.child.id()
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        self.child.try_wait().context("poll unhealthy lock owner")
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

fn daemon_pid_from(store_root: &Path) -> Result<u32> {
    let raw =
        std::fs::read_to_string(store_root.join("daemon.json")).context("daemon discovery")?;
    let json: serde_json::Value = serde_json::from_str(&raw).context("valid daemon discovery")?;
    let pid = json["pid"].as_u64().context("daemon pid")?;
    u32::try_from(pid).context("pid fits u32")
}

fn run_daemon_command(store_root: &Path, command: &str) -> Result<Output> {
    std::process::Command::new(env!("CARGO_BIN_EXE_git-tools"))
        .args(["daemon", command])
        .env("GIT_TOOLS_DATA_DIR", store_root)
        .output()
        .with_context(|| format!("run daemon {command}"))
}

#[test]
fn invalid_discovery_records_are_treated_as_not_running() -> Result<()> {
    let store = tempfile::tempdir().context("temporary daemon store")?;

    for record in [
        r#"{"port":0,"pid":99}"#,
        r#"{"port":4321,"pid":0}"#,
        "not JSON",
    ] {
        std::fs::write(store.path().join("daemon.json"), record)
            .context("write invalid discovery record")?;

        let output = run_daemon_command(store.path(), "status")?;

        assert!(output.status.success(), "record: {record}");
        assert_eq!(
            output.stdout, b"gtl-daemon not running\n",
            "record: {record}"
        );
        assert!(output.stderr.is_empty(), "record: {record}");
    }
    Ok(())
}

#[test]
fn stale_discovery_record_without_an_owner_is_not_running() -> Result<()> {
    let store = tempfile::tempdir().context("temporary daemon store")?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").context("bind stale port")?;
    let port = listener.local_addr().context("stale port address")?.port();
    std::fs::write(
        store.path().join("daemon.json"),
        format!(r#"{{"port":{port},"pid":99}}"#),
    )
    .context("write stale discovery record")?;

    let output = run_daemon_command(store.path(), "status")?;

    assert!(output.status.success());
    assert_eq!(output.stdout, b"gtl-daemon not running\n");
    assert!(output.stderr.is_empty());
    Ok(())
}

#[test]
fn unhealthy_lock_owner_blocks_restart_without_being_killed() -> Result<()> {
    let store = tempfile::tempdir().context("temporary daemon store")?;
    let mut owner = UnhealthyLockOwnerGuard::spawn(store.path())?;
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .context("reserve unused port")?
        .local_addr()
        .context("reserved port address")?
        .port();
    std::fs::write(
        store.path().join("daemon.json"),
        format!(r#"{{"port":{port},"pid":{}}}"#, owner.id()),
    )
    .context("write unhealthy discovery document")?;

    let status_output = run_daemon_command(store.path(), "status")?;
    assert_eq!(status_output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&status_output.stderr)
            .contains("daemon lock is owned but no healthy daemon responded"),
        "stderr: {}",
        String::from_utf8_lossy(&status_output.stderr)
    );
    assert!(
        owner.try_wait()?.is_none(),
        "status must not kill the lock owner"
    );

    let started_at = Instant::now();
    let output = run_daemon_command(store.path(), "restart")?;

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("daemon lock is owned but no healthy daemon responded"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(started_at.elapsed() < Duration::from_secs(7));
    assert!(
        owner.try_wait()?.is_none(),
        "the CLI must not kill a PID from daemon.json"
    );
    assert_eq!(daemon_pid_from(store.path())?, owner.id());
    Ok(())
}

#[test]
#[ignore = "subprocess helper"]
fn daemon_lock_owner_process() -> Result<()> {
    let Some(store_root) = std::env::var_os("GIT_TOOLS_TEST_DAEMON_LOCK_OWNER") else {
        return Ok(());
    };
    let store_root = PathBuf::from(store_root);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store_root.join("daemon.lock"))
        .context("open daemon lock")?;
    lock.lock().context("own daemon lock")?;
    std::fs::write(store_root.join("lock-owner.ready"), b"ready")
        .context("publish lock-owner readiness")?;
    std::thread::sleep(Duration::from_secs(30));
    Ok(())
}
