//! The daemon client: the swap-later `Backend` trait and its localhost-HTTP
//! implementation (discovery via the port file, exe-identity handshake with
//! restart-on-mismatch, autostart via the PAL's detached spawn).

use std::{
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use application::diffs::{
    render_diff::RenderDiff, render_diff_all::RenderDiffAll,
    render_diff_subrepos::RenderDiffSubrepos, render_merge_diff::RenderMergeDiff,
    render_squash_preview::RenderSquashPreview,
};
use contracts::{
    diffs::RenderDiffData,
    envelope::Envelope,
    live_views::{SaveLiveViewData, SaveLiveViewRequest},
    managed::{PullAllRequest, PushAllRequest, SyncData},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// The swap-later seam: `gtl diff` and its siblings render through whatever
/// backend they are handed. The production impl talks to the resident daemon
/// over localhost HTTP; tests substitute an in-memory fake that only overrides
/// the method(s) it exercises.
pub trait Backend {
    /// Render a diff preview, returning the service-composed wire envelope.
    ///
    /// # Errors
    /// Returns an error only on transport/parse failure — an error *outcome* is
    /// carried inside the returned [`Envelope`], not as `Err`.
    fn render_diff(&self, _req: &RenderDiff) -> anyhow::Result<Envelope<RenderDiffData>> {
        unimplemented!("render_diff")
    }

    /// Render a merge-diff preview, returning the service-composed wire envelope.
    ///
    /// # Errors
    /// Returns an error only on transport/parse failure — an error *outcome* is
    /// carried inside the returned [`Envelope`], not as `Err`.
    fn render_merge_diff(
        &self,
        _req: &RenderMergeDiff,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
        unimplemented!("render_merge_diff")
    }

    /// Render a squash-preview artifact, returning the service-composed wire envelope.
    ///
    /// # Errors
    /// Returns an error only on transport/parse failure — an error *outcome* is
    /// carried inside the returned [`Envelope`], not as `Err`.
    fn render_squash_preview(
        &self,
        _req: &RenderSquashPreview,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
        unimplemented!("render_squash_preview")
    }

    /// Render a diff-subrepos artifact, returning the service-composed wire envelope.
    ///
    /// # Errors
    /// Returns an error only on transport/parse failure — an error *outcome* is
    /// carried inside the returned [`Envelope`], not as `Err`.
    fn render_diff_subrepos(
        &self,
        _req: &RenderDiffSubrepos,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
        unimplemented!("render_diff_subrepos")
    }

    /// Render a diff-all artifact, returning the service-composed wire envelope.
    ///
    /// # Errors
    /// Returns an error only on transport/parse failure — an error *outcome* is
    /// carried inside the returned [`Envelope`], not as `Err`.
    fn render_diff_all(&self, _req: &RenderDiffAll) -> anyhow::Result<Envelope<RenderDiffData>> {
        unimplemented!("render_diff_all")
    }

    /// Push every managed repo, returning the service-composed wire envelope.
    ///
    /// # Errors
    /// Returns an error only on transport/parse failure — an error *outcome* is
    /// carried inside the returned [`Envelope`], not as `Err`.
    fn push_all(&self, _req: &PushAllRequest) -> anyhow::Result<Envelope<SyncData>> {
        unimplemented!("push_all")
    }

    /// Pull every managed repo, returning the service-composed wire envelope.
    ///
    /// # Errors
    /// Returns an error only on transport/parse failure — an error *outcome* is
    /// carried inside the returned [`Envelope`], not as `Err`.
    fn pull_all(&self, _req: &PullAllRequest) -> anyhow::Result<Envelope<SyncData>> {
        unimplemented!("pull_all")
    }

    /// Validate + persist a live-view source, returning the service-composed wire envelope.
    ///
    /// # Errors
    /// Returns an error only on transport/parse failure — a rejection is carried inside the
    /// returned [`Envelope`], not as `Err`.
    fn save_live_view(
        &self,
        _req: &SaveLiveViewRequest,
    ) -> anyhow::Result<Envelope<SaveLiveViewData>> {
        unimplemented!("save_live_view")
    }
}

/// The `/health` identity payload, deserialized from a running daemon.
#[derive(Debug, Deserialize)]
struct Health {
    pid: u32,
    version: String,
    exe_len: u64,
    exe_modified_ms: u64,
}

/// The discovery record under the store root (`<store_root>/daemon.json`).
#[derive(Debug, Deserialize)]
struct PortFile {
    port: u16,
    pid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaemonLockOwnership {
    Free,
    Owned,
}

struct DaemonLockStartup {
    file: File,
}

impl Drop for DaemonLockStartup {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

enum DaemonOwnerWait {
    Healthy { port: u16, health: Health },
    Released,
}

enum DaemonReplacementOutcome {
    Healthy { port: u16, health: Health },
    Released,
}

/// A running daemon's status, for `gtl daemon status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatus {
    /// Listening localhost port.
    pub port: u16,
    /// Process identifier reported by the health endpoint.
    pub pid: u32,
    /// Daemon package version.
    pub version: String,
}

/// Result of inspecting the resident daemon and its ownership lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonStatusOutcome {
    /// A healthy daemon answered its identity endpoint.
    Running(DaemonStatus),
    /// No healthy daemon or lock owner exists.
    NotRunning,
    /// A process owns the daemon lock but does not answer health probes.
    OwnedUnhealthy,
}

/// Result of asking the resident daemon to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonStopOutcome {
    /// A healthy daemon stopped and released ownership.
    Stopped,
    /// No healthy process or lock owner existed.
    NotRunning,
}

/// The localhost-HTTP backend: a base URL resolved by [`HttpBackend::ensure_daemon`]
/// plus a blocking reqwest client (connect-timeout only — a large diff may take a
/// while to render).
pub struct HttpBackend {
    base_url: String,
    http: reqwest::blocking::Client,
}

impl HttpBackend {
    /// Discover-or-start the daemon and connect to it.
    ///
    /// Discovery order: a pinned `GIT_TOOLS_DAEMON_PORT` → the port file →
    /// autostart. A live daemon whose exe identity matches the sibling binary is
    /// reused; a mismatch (e.g. after `just update`) is asked to shut down and
    /// respawned so the pinned port frees up before the fresh bind.
    ///
    /// # Errors
    /// Returns an error only when a daemon cannot be brought up (spawn failure or
    /// a 5 s startup timeout) or the HTTP client cannot be built.
    pub fn ensure_daemon() -> anyhow::Result<Self> {
        let bin = daemon_bin();
        let mut stale_pid = read_port_file().map(|port_file| port_file.pid);

        if let Some((port, health)) = daemon_candidate_or_wait()? {
            if identity_matches(&health, &bin) {
                return Self::connect(port);
            }
            stale_pid = Some(health.pid);
            match replace_healthy_daemon(port, health.pid, &bin)? {
                DaemonReplacementOutcome::Healthy { port, .. } => return Self::connect(port),
                DaemonReplacementOutcome::Released => {}
            }
        }

        Self::spawn_and_connect(&bin, stale_pid).map(|(backend, _)| backend)
    }

    /// Spawn `gtl-daemon` detached and poll for it to publish a fresh, matching
    /// port file.
    fn spawn_and_connect(
        bin: &Path,
        stale_pid: Option<u32>,
    ) -> anyhow::Result<(Self, DaemonStatus)> {
        gtl_platform::spawn_detached(bin, &[])
            .with_context(|| format!("spawn gtl-daemon ({})", bin.display()))?;

        let Some((port, health)) = wait_for_matching_daemon(bin, stale_pid, SPAWN_DEADLINE) else {
            anyhow::bail!(
                "gtl-daemon did not start within 5s (binary: {})",
                bin.display()
            );
        };
        let backend = Self::connect(port)?;
        let status = DaemonStatus {
            port,
            pid: health.pid,
            version: health.version,
        };
        Ok((backend, status))
    }

    /// Build the blocking client for a resolved port (connect-timeout only).
    fn connect(port: u16) -> anyhow::Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(1))
            .build()
            .context("build daemon HTTP client")?;
        Ok(Self {
            base_url: format!("http://127.0.0.1:{port}"),
            http,
        })
    }

    /// POST `req` as JSON to `path` and parse the response body as `Res`. A 400/500
    /// carries an error envelope, so the body is deserialized regardless of status.
    fn post_json<Req: Serialize, Res: DeserializeOwned>(
        &self,
        path: &str,
        req: &Req,
    ) -> anyhow::Result<Res> {
        self.http
            .post(format!("{}{path}", self.base_url))
            .json(req)
            .send()
            .context("daemon request failed")?
            .json::<Res>()
            .context("daemon request failed")
    }
}

impl Backend for HttpBackend {
    fn render_diff(&self, req: &RenderDiff) -> anyhow::Result<Envelope<RenderDiffData>> {
        self.post_json("/diffs/render", req)
    }

    fn render_merge_diff(&self, req: &RenderMergeDiff) -> anyhow::Result<Envelope<RenderDiffData>> {
        self.post_json("/diffs/merge", req)
    }

    fn render_squash_preview(
        &self,
        req: &RenderSquashPreview,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
        self.post_json("/diffs/squash-preview", req)
    }

    fn render_diff_subrepos(
        &self,
        req: &RenderDiffSubrepos,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
        self.post_json("/diffs/subrepos", req)
    }

    fn render_diff_all(&self, req: &RenderDiffAll) -> anyhow::Result<Envelope<RenderDiffData>> {
        self.post_json("/diffs/all", req)
    }

    fn push_all(&self, req: &PushAllRequest) -> anyhow::Result<Envelope<SyncData>> {
        self.post_json("/managed/push-all", req)
    }

    fn pull_all(&self, req: &PullAllRequest) -> anyhow::Result<Envelope<SyncData>> {
        self.post_json("/managed/pull-all", req)
    }

    fn save_live_view(
        &self,
        req: &SaveLiveViewRequest,
    ) -> anyhow::Result<Envelope<SaveLiveViewData>> {
        self.post_json("/live-views/save", req)
    }
}

/// Inspect the running daemon and its ownership lock.
///
/// # Errors
///
/// Returns an error when lock ownership cannot be inspected.
pub fn daemon_status() -> anyhow::Result<DaemonStatusOutcome> {
    if let Some((port, health)) = daemon_candidate(HEALTH_TIMEOUT) {
        return Ok(DaemonStatusOutcome::Running(status_from(port, health)));
    }
    match daemon_lock_ownership()? {
        DaemonLockOwnership::Free => Ok(DaemonStatusOutcome::NotRunning),
        DaemonLockOwnership::Owned => Ok(DaemonStatusOutcome::OwnedUnhealthy),
    }
}

/// Ask a running daemon to exit.
///
/// # Errors
///
/// Returns an error when startup cannot be quiesced, an unhealthy owner keeps
/// the lock, the daemon misses the stop deadline, or lock ownership cannot be inspected.
pub fn daemon_stop() -> anyhow::Result<DaemonStopOutcome> {
    let started_at = Instant::now();
    let Some(_daemon_lock_startup) = acquire_daemon_lock_startup(SPAWN_DEADLINE)? else {
        anyhow::bail!("daemon startup did not release daemon.start.lock within 5s");
    };
    let budget = SPAWN_DEADLINE.saturating_sub(started_at.elapsed());
    let Some((port, health)) = daemon_candidate_or_wait_with_budget(budget)? else {
        return Ok(DaemonStopOutcome::NotRunning);
    };
    stop_healthy_daemon(port, health.pid)?;
    Ok(DaemonStopOutcome::Stopped)
}

/// Replace a healthy daemon, or start one when none owns the store.
///
/// # Errors
///
/// Returns an error when the current owner is unhealthy, does not stop within
/// budget, the replacement cannot be spawned, or it misses the startup budget.
pub fn daemon_restart() -> anyhow::Result<DaemonStatus> {
    let bin = daemon_bin();
    let mut stale_pid = read_port_file().map(|port_file| port_file.pid);
    if let Some((port, health)) = daemon_candidate_or_wait()? {
        stale_pid = Some(health.pid);
        match replace_healthy_daemon(port, health.pid, &bin)? {
            DaemonReplacementOutcome::Healthy { port, health } => {
                return Ok(status_from(port, health));
            }
            DaemonReplacementOutcome::Released => {}
        }
    }
    HttpBackend::spawn_and_connect(&bin, stale_pid).map(|(_, status)| status)
}

const HEALTH_TIMEOUT: Duration = Duration::from_millis(500);
const STOP_BUDGET: Duration = Duration::from_secs(2);
const SPAWN_DEADLINE: Duration = Duration::from_secs(5);
const SPAWN_POLL_STEP: Duration = Duration::from_millis(50);

/// The pinned port from `GIT_TOOLS_DAEMON_PORT`, if it parses.
fn pinned_port() -> Option<u16> {
    std::env::var("GIT_TOOLS_DAEMON_PORT")
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
}

/// The daemon binary: `gtl-daemon` next to the running CLI exe, else the bare
/// name for the OS PATH resolver (mirrors `viewer::resolve_viewer_bin`).
fn daemon_bin() -> PathBuf {
    let name = format!("gtl-daemon{}", std::env::consts::EXE_SUFFIX);
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join(&name);
        if sibling.is_file() {
            return sibling;
        }
    }
    PathBuf::from(name)
}

/// Read `<store_root>/daemon.json`, `None` when absent or malformed.
fn read_port_file() -> Option<PortFile> {
    let root = gtl_platform::paths::store_root().ok()?;
    let raw = std::fs::read_to_string(root.join("daemon.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

fn daemon_candidate(timeout: Duration) -> Option<(u16, Health)> {
    let deadline = Instant::now() + timeout;
    let port_pinned = pinned_port();
    let port_discovered = read_port_file().map(|port_file| port_file.port);
    match (port_pinned, port_discovered) {
        (Some(pinned), Some(discovered)) if pinned != discovered => {
            let timeout_pinned = timeout / 2;
            if !timeout_pinned.is_zero()
                && let Some(health) = health(pinned, timeout_pinned)
            {
                return Some((pinned, health));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                None
            } else {
                health(discovered, remaining).map(|health| (discovered, health))
            }
        }
        (Some(port), _) | (None, Some(port)) => health(port, timeout).map(|health| (port, health)),
        (None, None) => None,
    }
}

fn daemon_candidate_or_wait() -> anyhow::Result<Option<(u16, Health)>> {
    daemon_candidate_or_wait_with_budget(SPAWN_DEADLINE)
}

fn daemon_candidate_or_wait_with_budget(budget: Duration) -> anyhow::Result<Option<(u16, Health)>> {
    let started_at = Instant::now();
    let health_budget = budget.min(HEALTH_TIMEOUT);
    if !health_budget.is_zero()
        && let Some(candidate) = daemon_candidate(health_budget)
    {
        return Ok(Some(candidate));
    }
    match daemon_lock_ownership()? {
        DaemonLockOwnership::Free => Ok(None),
        DaemonLockOwnership::Owned => {
            let remaining = budget.saturating_sub(started_at.elapsed());
            match wait_for_daemon_owner(remaining)? {
                DaemonOwnerWait::Healthy { port, health } => Ok(Some((port, health))),
                DaemonOwnerWait::Released => Ok(None),
            }
        }
    }
}

fn wait_for_daemon_owner(budget: Duration) -> anyhow::Result<DaemonOwnerWait> {
    let deadline = Instant::now() + budget;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero()
            && let Some((port, health)) = daemon_candidate(remaining.min(HEALTH_TIMEOUT))
        {
            return Ok(DaemonOwnerWait::Healthy { port, health });
        }
        if daemon_lock_ownership()? == DaemonLockOwnership::Free {
            return Ok(DaemonOwnerWait::Released);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            anyhow::bail!(
                "daemon lock is owned but no healthy daemon responded before the startup deadline"
            );
        }
        std::thread::sleep(remaining.min(SPAWN_POLL_STEP));
    }
}

fn wait_for_matching_daemon(
    bin: &Path,
    stale_pid: Option<u32>,
    budget: Duration,
) -> Option<(u16, Health)> {
    let deadline = Instant::now() + budget;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        if let Some((port, health)) = daemon_candidate(remaining.min(HEALTH_TIMEOUT))
            && stale_pid != Some(health.pid)
            && identity_matches(&health, bin)
        {
            return Some((port, health));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        std::thread::sleep(remaining.min(SPAWN_POLL_STEP));
    }
}

fn daemon_lock_ownership() -> anyhow::Result<DaemonLockOwnership> {
    let store_root = gtl_platform::paths::store_root().context("resolve daemon store root")?;
    std::fs::create_dir_all(&store_root)
        .with_context(|| format!("create daemon store root at {}", store_root.display()))?;
    let path = store_root.join("daemon.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("open daemon lock at {}", path.display()))?;
    match file.try_lock_shared() {
        Ok(()) => {
            file.unlock()
                .with_context(|| format!("unlock daemon ownership probe at {}", path.display()))?;
            Ok(DaemonLockOwnership::Free)
        }
        Err(TryLockError::WouldBlock) => Ok(DaemonLockOwnership::Owned),
        Err(TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("inspect daemon ownership at {}", path.display()))
        }
    }
}

fn acquire_daemon_lock_startup(budget: Duration) -> anyhow::Result<Option<DaemonLockStartup>> {
    let store_root = gtl_platform::paths::store_root().context("resolve daemon store root")?;
    std::fs::create_dir_all(&store_root)
        .with_context(|| format!("create daemon store root at {}", store_root.display()))?;
    let path = store_root.join("daemon.start.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("open daemon startup lock at {}", path.display()))?;
    let deadline = Instant::now() + budget;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Some(DaemonLockStartup { file })),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => {
                return Err(error).with_context(|| {
                    format!("lock daemon startup election at {}", path.display())
                });
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        std::thread::sleep(remaining.min(SPAWN_POLL_STEP));
    }
}

/// `GET /health` with an overall `timeout`; `None` on any failure.
fn health(port: u16, timeout: Duration) -> Option<Health> {
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .ok()?;
    client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .ok()?
        .json::<Health>()
        .ok()
}

/// Whether the file at `bin` has the same length + mtime-ms the daemon reported
/// (mirrors `daemon::lifecycle::ExeIdentity::of`).
fn identity_matches(health: &Health, bin: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(bin) else {
        return false;
    };
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    meta.len() == health.exe_len && modified_ms == health.exe_modified_ms
}

fn status_from(port: u16, health: Health) -> DaemonStatus {
    DaemonStatus {
        port,
        pid: health.pid,
        version: health.version,
    }
}

/// Best-effort `POST /shutdown` to a daemon on `port`.
fn shutdown(port: u16, timeout: Duration) {
    if timeout.is_zero() {
        return;
    }
    if let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
    {
        let _ = client
            .post(format!("http://127.0.0.1:{port}/shutdown"))
            .send();
    }
}

/// Poll until `port` stops reporting `pid`, or `budget` elapses.
fn wait_until_pid_gone(port: u16, pid: u32, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        if health(port, remaining.min(Duration::from_millis(300)))
            .is_none_or(|health| health.pid != pid)
        {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        std::thread::sleep(remaining.min(SPAWN_POLL_STEP));
    }
}

fn wait_for_replacement(
    bin: &Path,
    pid_stale: u32,
    budget: Duration,
) -> anyhow::Result<Option<(u16, Health)>> {
    let deadline = Instant::now() + budget;
    loop {
        if daemon_lock_ownership()? == DaemonLockOwnership::Free {
            return Ok(None);
        }
        if read_port_file().is_some_and(|port_file| port_file.pid != pid_stale) {
            let Some(candidate) = wait_for_matching_daemon(bin, Some(pid_stale), SPAWN_DEADLINE)
            else {
                anyhow::bail!("gtl-daemon replacement did not become healthy within 5s");
            };
            return Ok(Some(candidate));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            anyhow::bail!(
                "gtl-daemon did not release daemon.lock or yield to a healthy replacement within 2s"
            );
        }
        if let Some((port, health)) = daemon_candidate(remaining.min(HEALTH_TIMEOUT))
            && health.pid != pid_stale
            && identity_matches(&health, bin)
        {
            return Ok(Some((port, health)));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            anyhow::bail!(
                "gtl-daemon did not release daemon.lock or yield to a healthy replacement within 2s"
            );
        }
        std::thread::sleep(remaining.min(SPAWN_POLL_STEP));
    }
}

fn wait_until_lock_free(budget: Duration) -> anyhow::Result<bool> {
    let deadline = Instant::now() + budget;
    loop {
        if daemon_lock_ownership()? == DaemonLockOwnership::Free {
            return Ok(true);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        std::thread::sleep(remaining.min(SPAWN_POLL_STEP));
    }
}

fn stop_daemon_process(port: u16, pid: u32) -> anyhow::Result<Duration> {
    let started_at = Instant::now();
    shutdown(port, STOP_BUDGET);
    let remaining = STOP_BUDGET.saturating_sub(started_at.elapsed());
    if !wait_until_pid_gone(port, pid, remaining) {
        anyhow::bail!("gtl-daemon did not stop responding within 2s");
    }
    Ok(STOP_BUDGET.saturating_sub(started_at.elapsed()))
}

fn stop_healthy_daemon(port: u16, pid: u32) -> anyhow::Result<()> {
    let remaining = stop_daemon_process(port, pid)?;
    if !wait_until_lock_free(remaining)? {
        anyhow::bail!("gtl-daemon did not release daemon.lock within 2s");
    }
    Ok(())
}

fn replace_healthy_daemon(
    port: u16,
    pid: u32,
    bin: &Path,
) -> anyhow::Result<DaemonReplacementOutcome> {
    let remaining = stop_daemon_process(port, pid)?;
    match wait_for_replacement(bin, pid, remaining)? {
        Some((port, health)) => Ok(DaemonReplacementOutcome::Healthy { port, health }),
        None => Ok(DaemonReplacementOutcome::Released),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Read as _,
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use super::*;

    #[test]
    fn identity_matches_real_file_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gtl-daemon-fake");
        std::fs::write(&path, b"0123456789").unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        let ms = u64::try_from(
            meta.modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .expect("mtime in ms fits u64 for eons");

        let health = Health {
            pid: 1,
            version: "0.1.0".into(),
            exe_len: meta.len(),
            exe_modified_ms: ms,
        };
        assert!(identity_matches(&health, &path));

        let mismatched = Health {
            exe_len: meta.len() + 1,
            ..health
        };
        assert!(!identity_matches(&mismatched, &path));
    }

    #[test]
    fn identity_does_not_match_a_missing_file() {
        let health = Health {
            pid: 1,
            version: "0.1.0".into(),
            exe_len: 10,
            exe_modified_ms: 0,
        };
        assert!(!identity_matches(&health, Path::new("/no/such/gtl-daemon")));
    }

    #[test]
    fn port_file_json_parses() {
        let pf: PortFile = serde_json::from_str(r#"{"port":4321,"pid":99}"#).unwrap();
        assert_eq!(pf.port, 4321);
        assert_eq!(pf.pid, 99);
    }

    #[test]
    fn daemon_bin_name_uses_the_platform_exe_suffix() {
        let bin = daemon_bin();
        let name = bin.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, format!("gtl-daemon{}", std::env::consts::EXE_SUFFIX));
    }

    #[test]
    fn daemon_request_transport_failure_is_not_replayed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let connections_count = Arc::new(AtomicUsize::new(0));
        let connections_count_server = Arc::clone(&connections_count);
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut first_connection_at: Option<Instant> = None;
            while Instant::now() < deadline
                && first_connection_at.is_none_or(|at| at.elapsed() < Duration::from_millis(300))
            {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        connections_count_server.fetch_add(1, Ordering::SeqCst);
                        first_connection_at.get_or_insert_with(Instant::now);
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut request = [0_u8; 4096];
                        let _ = stream.read(&mut request);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("accept daemon request: {error}"),
                }
            }
        });

        let backend = HttpBackend::connect(port).unwrap();
        let result = backend.post_json::<_, serde_json::Value>(
            "/diffs/render",
            &serde_json::json!({"request": "once"}),
        );

        assert!(
            result.is_err(),
            "a closed response must be a transport error"
        );
        server.join().unwrap();
        assert_eq!(connections_count.load(Ordering::SeqCst), 1);
    }
}
