use std::{
    fs::{self, File},
    io::{Seek as _, SeekFrom, Write as _},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use gtl_benchmarks::{
    grpc_transport::GhzMeasurement,
    server_highlighting::{parse_process_rss, read_process_rss},
};
use gtl_local_auth::CapabilityToken;
use serde::Serialize;

use super::environment::DriverConfig;

const GHZ_OUTPUT_BYTES_MAX: u64 = 16 * 1_024 * 1_024;
const GHZ_DIAGNOSTIC_BYTES_MAX: usize = 64 * 1_024;

#[derive(Serialize)]
struct GhzMetadata {
    authorization: String,
}

pub struct GhzRun {
    pub measurement: GhzMeasurement,
    pub peak_server_and_client_rss_bytes: u64,
}

pub fn run_ghz(
    config: &DriverConfig,
    sandbox: &Path,
    target: &str,
    capability: &CapabilityToken,
    server_process_id: u32,
) -> Result<GhzRun> {
    let mut metadata =
        tempfile::NamedTempFile::new_in(sandbox).context("create temporary ghz metadata file")?;
    serde_json::to_writer(
        &mut metadata,
        &GhzMetadata {
            authorization: format!("Bearer {}", capability.expose_secret()),
        },
    )
    .context("encode temporary ghz metadata")?;
    metadata.flush().context("flush temporary ghz metadata")?;

    let output =
        tempfile::NamedTempFile::new_in(sandbox).context("create temporary ghz result file")?;
    let stdout_path = sandbox.join("ghz.stdout.log");
    let stderr_path = sandbox.join("ghz.stderr.log");
    let stdout =
        File::create(&stdout_path).with_context(|| format!("create {}", stdout_path.display()))?;
    let stderr =
        File::create(&stderr_path).with_context(|| format!("create {}", stderr_path.display()))?;
    let mut child = Command::new(&config.ghz_binary)
        .arg(format!("--config={}", config.ghz_config_path.display()))
        .arg(format!("--metadata-file={}", metadata.path().display()))
        .arg("--format=json")
        .arg(format!("--output={}", output.path().display()))
        .arg(target)
        .current_dir(&config.repository_root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .context("start ghz transport workload")?;
    let sampler =
        CombinedRssSampler::start(server_process_id, child.id(), config.rss_sample_interval)?;
    let status = wait_for_ghz(&mut child, config.ghz_wall_timeout)?;
    let peak_server_and_client_rss_bytes = sampler.finish()?;
    if !status.success() {
        let stdout = bounded_diagnostic(&stdout_path)?;
        let stderr = bounded_diagnostic(&stderr_path)?;
        bail!(
            "ghz failed with exit {}: stdout: {stdout}; stderr: {stderr}",
            status.code().unwrap_or(-1)
        );
    }

    let metadata = fs::metadata(output.path()).context("read ghz result metadata")?;
    ensure!(
        metadata.len() <= GHZ_OUTPUT_BYTES_MAX,
        "ghz result exceeds {GHZ_OUTPUT_BYTES_MAX} bytes"
    );
    let mut result = output.reopen().context("open ghz result")?;
    result
        .seek(SeekFrom::Start(0))
        .context("rewind ghz result")?;
    let measurement =
        serde_json::from_reader(std::io::BufReader::new(result)).context("decode ghz result")?;
    Ok(GhzRun {
        measurement,
        peak_server_and_client_rss_bytes,
    })
}

fn wait_for_ghz(
    child: &mut std::process::Child,
    wall_timeout: Duration,
) -> Result<std::process::ExitStatus> {
    const POLL_INTERVAL: Duration = Duration::from_millis(10);
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().context("poll ghz transport workload")? {
            return Ok(status);
        }
        if started.elapsed() >= wall_timeout {
            child.kill().context("terminate overdue ghz workload")?;
            let status = child.wait().context("reap overdue ghz workload")?;
            bail!(
                "ghz exceeded the {}-second wall-time bound (exit {})",
                wall_timeout.as_secs(),
                status.code().unwrap_or(-1)
            );
        }
        thread::sleep(POLL_INTERVAL);
    }
}

struct CombinedRssSampler {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<Result<u64>>>,
}

impl CombinedRssSampler {
    fn start(server_process_id: u32, client_process_id: u32, interval: Duration) -> Result<Self> {
        let initial = combined_rss(server_process_id, client_process_id)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("grpc-transport-rss".to_owned())
            .spawn(move || {
                let mut peak = initial;
                while !thread_stop.load(Ordering::Acquire) {
                    peak = peak.max(combined_rss(server_process_id, client_process_id)?);
                    thread::sleep(interval);
                }
                Ok(peak.max(combined_rss_without_client(server_process_id)?))
            })
            .context("start gRPC transport RSS sampler")?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    fn finish(mut self) -> Result<u64> {
        self.stop.store(true, Ordering::Release);
        self.thread
            .take()
            .context("gRPC transport RSS sampler thread is missing")?
            .join()
            .map_err(|_| anyhow::anyhow!("gRPC transport RSS sampler panicked"))?
    }
}

impl Drop for CombinedRssSampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn combined_rss(server_process_id: u32, client_process_id: u32) -> Result<u64> {
    let server = read_process_rss(server_process_id).context("sample gtl-server RSS")?;
    let client = read_optional_client_rss(client_process_id)?;
    server
        .current
        .checked_add(client)
        .context("combined gtl-server and ghz RSS overflows")
}

fn read_optional_client_rss(client_process_id: u32) -> Result<u64> {
    let path = std::path::PathBuf::from(format!("/proc/{client_process_id}/status"));
    let status = match fs::read_to_string(&path) {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    if !status.lines().any(|line| line.starts_with("VmRSS:")) {
        return Ok(0);
    }
    match parse_process_rss(&status) {
        Ok(sample) => Ok(sample.current),
        Err(error) => Err(error).context("parse ghz RSS"),
    }
}

fn combined_rss_without_client(server_process_id: u32) -> Result<u64> {
    read_process_rss(server_process_id)
        .context("sample final gtl-server RSS")
        .map(|sample| sample.current)
}

fn bounded_diagnostic(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let end = bytes.len().min(GHZ_DIAGNOSTIC_BYTES_MAX);
    let suffix = if bytes.len() > end {
        "...[truncated]"
    } else {
        ""
    };
    Ok(format!(
        "{}{suffix}",
        String::from_utf8_lossy(&bytes[..end]).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_timeout_terminates_and_reaps_ghz_process() {
        let mut child = Command::new("/usr/bin/sleep").arg("10").spawn().unwrap();

        let error = wait_for_ghz(&mut child, Duration::from_millis(1)).unwrap_err();

        assert!(error.to_string().contains("wall-time bound"));
        assert!(child.try_wait().unwrap().is_some());
    }
}
