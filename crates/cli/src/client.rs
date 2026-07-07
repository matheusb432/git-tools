//! The daemon client: the swap-later `Backend` trait and its localhost-HTTP
//! implementation (discovery via the port file, exe-identity handshake with
//! restart-on-mismatch, autostart via the PAL's detached spawn).

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use contracts::{
    diffs::{
        RenderDiffAllRequest, RenderDiffData, RenderDiffRequest, RenderDiffSubreposRequest,
        RenderMergeDiffRequest, RenderSquashPreviewRequest,
    },
    envelope::Envelope,
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
    fn render_diff(&self, _req: &RenderDiffRequest) -> anyhow::Result<Envelope<RenderDiffData>> {
        unimplemented!("render_diff")
    }

    /// Render a merge-diff preview, returning the service-composed wire envelope.
    ///
    /// # Errors
    /// Returns an error only on transport/parse failure — an error *outcome* is
    /// carried inside the returned [`Envelope`], not as `Err`.
    fn render_merge_diff(
        &self,
        _req: &RenderMergeDiffRequest,
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
        _req: &RenderSquashPreviewRequest,
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
        _req: &RenderDiffSubreposRequest,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
        unimplemented!("render_diff_subrepos")
    }

    /// Render a diff-all artifact, returning the service-composed wire envelope.
    ///
    /// # Errors
    /// Returns an error only on transport/parse failure — an error *outcome* is
    /// carried inside the returned [`Envelope`], not as `Err`.
    fn render_diff_all(
        &self,
        _req: &RenderDiffAllRequest,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
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

/// A running daemon's status, for `gtl daemon status`.
pub struct DaemonStatus {
    pub port: u16,
    pub pid: u32,
    pub version: String,
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
        let pre = read_port_file();

        if let Some(port) = pinned_port() {
            if let Some(health) = health(port, HEALTH_TIMEOUT) {
                if identity_matches(&health, &bin) {
                    return Self::connect(port);
                }
                // Mismatched identity on the pinned port: it must die before the
                // fresh daemon can bind the same port.
                shutdown(port);
                wait_until_dead(port, STOP_BUDGET);
            }
        } else if let Some(pf) = read_port_file()
            && let Some(health) = health(pf.port, HEALTH_TIMEOUT)
        {
            if identity_matches(&health, &bin) {
                return Self::connect(pf.port);
            }
            shutdown(pf.port);
            wait_until_dead(pf.port, STOP_BUDGET);
        }

        Self::spawn_and_connect(&bin, pre.map(|p| p.pid))
    }

    /// Spawn `gtl-daemon` detached and poll for it to publish a fresh, matching
    /// port file.
    fn spawn_and_connect(bin: &Path, stale_pid: Option<u32>) -> anyhow::Result<Self> {
        gtl_platform::spawn_detached(bin, &[])
            .with_context(|| format!("spawn gtl-daemon ({})", bin.display()))?;

        let deadline = Instant::now() + SPAWN_DEADLINE;
        while Instant::now() < deadline {
            if let Some(pf) = read_port_file()
                && stale_pid != Some(pf.pid)
                && let Some(health) = health(pf.port, HEALTH_TIMEOUT)
                && identity_matches(&health, bin)
            {
                return Self::connect(pf.port);
            }
            std::thread::sleep(SPAWN_POLL_STEP);
        }
        anyhow::bail!(
            "gtl-daemon did not start within 5s (binary: {})",
            bin.display()
        )
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
    fn render_diff(&self, req: &RenderDiffRequest) -> anyhow::Result<Envelope<RenderDiffData>> {
        self.post_json("/diffs/render", req)
    }

    fn render_merge_diff(
        &self,
        req: &RenderMergeDiffRequest,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
        self.post_json("/diffs/merge", req)
    }

    fn render_squash_preview(
        &self,
        req: &RenderSquashPreviewRequest,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
        self.post_json("/diffs/squash-preview", req)
    }

    fn render_diff_subrepos(
        &self,
        req: &RenderDiffSubreposRequest,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
        self.post_json("/diffs/subrepos", req)
    }

    fn render_diff_all(
        &self,
        req: &RenderDiffAllRequest,
    ) -> anyhow::Result<Envelope<RenderDiffData>> {
        self.post_json("/diffs/all", req)
    }

    fn push_all(&self, req: &PushAllRequest) -> anyhow::Result<Envelope<SyncData>> {
        self.post_json("/managed/push-all", req)
    }

    fn pull_all(&self, req: &PullAllRequest) -> anyhow::Result<Envelope<SyncData>> {
        self.post_json("/managed/pull-all", req)
    }
}

/// Report a running daemon's identity, or `None` when nothing answers.
pub fn daemon_status() -> Option<DaemonStatus> {
    let pf = read_port_file()?;
    let health = health(pf.port, HEALTH_TIMEOUT)?;
    Some(DaemonStatus {
        port: pf.port,
        pid: health.pid,
        version: health.version,
    })
}

/// Ask a running daemon to exit; `false` when nothing was running.
pub fn daemon_stop() -> bool {
    let Some(pf) = read_port_file() else {
        return false;
    };
    if health(pf.port, HEALTH_TIMEOUT).is_none() {
        return false;
    }
    shutdown(pf.port);
    wait_until_dead(pf.port, STOP_BUDGET);
    true
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

/// Best-effort `POST /shutdown` to a daemon on `port`.
fn shutdown(port: u16) {
    if let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(STOP_BUDGET)
        .build()
    {
        let _ = client
            .post(format!("http://127.0.0.1:{port}/shutdown"))
            .send();
    }
}

/// Poll until `port` stops answering `/health`, or `budget` elapses.
fn wait_until_dead(port: u16, budget: Duration) {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if health(port, Duration::from_millis(300)).is_none() {
            return;
        }
        std::thread::sleep(SPAWN_POLL_STEP);
    }
}

#[cfg(test)]
mod tests {
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
}
