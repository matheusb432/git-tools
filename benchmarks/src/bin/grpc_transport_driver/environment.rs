use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, ensure};
use gtl_benchmarks::grpc_transport::{
    GrpcTransportProtocol, GrpcTransportResourceBounds, GrpcTransportRunner,
};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

const COMMAND_OUTPUT_BYTES_MAX: usize = 64 * 1_024;
const MEMORY_ATTRIBUTION: &str = "gtl-server and ghz processes";

pub struct DriverConfig {
    pub report_path: PathBuf,
    pub server_binary: PathBuf,
    pub ghz_binary: PathBuf,
    pub ghz_config_path: PathBuf,
    pub repository_root: PathBuf,
    pub launches: usize,
    pub source_commit: String,
    pub invocation: String,
    pub resource_bounds: GrpcTransportResourceBounds,
    pub rss_sample_interval: Duration,
    pub ghz_wall_timeout: Duration,
    pub protocol: GrpcTransportProtocol,
    pub runner: GrpcTransportRunner,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhzConfig {
    proto: PathBuf,
    call: String,
    total: u64,
    #[serde(rename = "skipFirst")]
    skip_first: u64,
    concurrency: u32,
    connections: u32,
    cpus: u32,
    insecure: bool,
    timeout: String,
    #[serde(rename = "disable-template-data")]
    disable_template_data: bool,
    data: toml::Table,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GhzWorkload {
    Benchmark,
    Smoke,
}

impl GhzWorkload {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Benchmark => "benchmark",
            Self::Smoke => "smoke",
        }
    }

    const fn request_counts(self) -> (u64, u64) {
        match self {
            Self::Benchmark => (10_200, 200),
            Self::Smoke => (120, 20),
        }
    }
}

impl DriverConfig {
    pub fn from_environment() -> Result<Self> {
        let launches = required_parse("GTL_GRPC_TRANSPORT_LAUNCHES")?;
        ensure!(launches > 0, "GTL_GRPC_TRANSPORT_LAUNCHES must be positive");
        let source_commit = required_string("GTL_GRPC_TRANSPORT_SOURCE_COMMIT")?;
        ensure!(
            source_commit.len() == 40 && source_commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "GTL_GRPC_TRANSPORT_SOURCE_COMMIT must be a full Git object ID"
        );
        let repository_root = required_path("GTL_GRPC_TRANSPORT_REPOSITORY_ROOT")?;
        ensure!(
            repository_root.is_dir(),
            "gRPC transport repository root is not a directory: {}",
            repository_root.display()
        );
        let server_binary = required_file("GTL_GRPC_TRANSPORT_SERVER_BINARY")?;
        let ghz_binary = required_file("GTL_GRPC_TRANSPORT_GHZ_BINARY")?;
        let ghz_config_path = required_file("GTL_GRPC_TRANSPORT_GHZ_CONFIG")?;
        let expected_ghz_version = required_string("GTL_GRPC_TRANSPORT_GHZ_VERSION")?;
        let ghz_version = command_output(&ghz_binary, &["--version"])?;
        ensure!(
            ghz_version == expected_ghz_version,
            "ghz version drifted: expected {expected_ghz_version}, found {ghz_version}"
        );
        let rss_sample_interval_microseconds =
            required_parse("GTL_GRPC_TRANSPORT_RSS_SAMPLE_INTERVAL_MICROSECONDS")?;
        ensure!(
            rss_sample_interval_microseconds > 0,
            "GTL_GRPC_TRANSPORT_RSS_SAMPLE_INTERVAL_MICROSECONDS must be positive"
        );
        let ghz_wall_timeout_seconds =
            required_parse("GTL_GRPC_TRANSPORT_GHZ_WALL_TIMEOUT_SECONDS")?;
        ensure!(
            ghz_wall_timeout_seconds > 0,
            "GTL_GRPC_TRANSPORT_GHZ_WALL_TIMEOUT_SECONDS must be positive"
        );
        let ghz_config_bytes = fs::read(&ghz_config_path)
            .with_context(|| format!("read ghz config {}", ghz_config_path.display()))?;
        let ghz_config_text = std::str::from_utf8(&ghz_config_bytes)
            .with_context(|| format!("ghz config is not UTF-8: {}", ghz_config_path.display()))?;
        let ghz_config: GhzConfig = toml::from_str(ghz_config_text)
            .with_context(|| format!("decode ghz config {}", ghz_config_path.display()))?;
        let workload = required_ghz_workload()?;
        validate_ghz_config(&ghz_config, workload)?;
        let proto_path = repository_root.join(&ghz_config.proto);
        let proto_bytes = fs::read(&proto_path)
            .with_context(|| format!("read benchmark proto {}", proto_path.display()))?;
        let measured_request_count = ghz_config
            .total
            .checked_sub(ghz_config.skip_first)
            .context("ghz warmup request count exceeds total request count")?;
        let protocol = GrpcTransportProtocol {
            independent_launches: launches,
            rpc: ghz_config.call,
            proto_sha256: sha256(&proto_bytes),
            ghz_config_sha256: sha256(&ghz_config_bytes),
            total_request_count: ghz_config.total,
            warmup_request_count: ghz_config.skip_first,
            measured_request_count,
            concurrency: ghz_config.concurrency,
            connections: ghz_config.connections,
            client_cpu_count: ghz_config.cpus,
            request_timeout_nanoseconds: parse_duration(&ghz_config.timeout)?
                .as_nanos()
                .try_into()
                .context("ghz request timeout exceeds u64 nanoseconds")?,
            ghz_wall_timeout_seconds,
            memory_attribution: MEMORY_ATTRIBUTION.to_owned(),
            rss_sample_interval_microseconds,
        };
        let resource_bounds = GrpcTransportResourceBounds {
            cpu_quota_percent: required_parse("GTL_GRPC_TRANSPORT_CPU_QUOTA_PERCENT")?,
            memory_max_bytes: required_parse("GTL_GRPC_TRANSPORT_MEMORY_MAX_BYTES")?,
            memory_swap_max_bytes: required_parse("GTL_GRPC_TRANSPORT_MEMORY_SWAP_MAX_BYTES")?,
            tasks_max: required_parse("GTL_GRPC_TRANSPORT_TASKS_MAX")?,
            process_niceness: required_parse("GTL_GRPC_TRANSPORT_PROCESS_NICENESS")?,
            cargo_jobs_max: required_parse("GTL_GRPC_TRANSPORT_CARGO_JOBS_MAX")?,
            wall_time_minutes: required_parse("GTL_GRPC_TRANSPORT_WALL_TIME_MINUTES")?,
            termination_grace_seconds: required_parse(
                "GTL_GRPC_TRANSPORT_TERMINATION_GRACE_SECONDS",
            )?,
        };
        Ok(Self {
            report_path: required_path("GTL_GRPC_TRANSPORT_REPORT_PATH")?,
            server_binary,
            ghz_binary,
            ghz_config_path,
            repository_root,
            launches,
            source_commit,
            invocation: required_string("GTL_GRPC_TRANSPORT_INVOCATION")?,
            resource_bounds,
            rss_sample_interval: Duration::from_micros(rss_sample_interval_microseconds),
            ghz_wall_timeout: Duration::from_secs(ghz_wall_timeout_seconds),
            protocol,
            runner: runner_environment(ghz_version)?,
        })
    }
}

fn validate_ghz_config(config: &GhzConfig, workload: GhzWorkload) -> Result<()> {
    let (total_request_count, warmup_request_count) = workload.request_counts();
    ensure!(
        config.proto == Path::new("crates/gtl-wire/proto/gtl/v1/settings.proto")
            && config.call == "gtl.v1.SettingsService.GetPushConfirmationRequirement"
            && config.total == total_request_count
            && config.skip_first == warmup_request_count
            && config.concurrency == 1
            && config.connections == 1
            && config.cpus == 1
            && config.insecure
            && config.timeout == "2s"
            && config.disable_template_data
            && config.data.is_empty(),
        "tracked {} ghz config drifted from its fixed sequential transport workload",
        workload.as_str()
    );
    Ok(())
}

fn required_ghz_workload() -> Result<GhzWorkload> {
    let workload = required_string("GTL_GRPC_TRANSPORT_WORKLOAD")?;
    match workload.as_str() {
        "benchmark" => Ok(GhzWorkload::Benchmark),
        "smoke" => Ok(GhzWorkload::Smoke),
        _ => Err(anyhow!(
            "GTL_GRPC_TRANSPORT_WORKLOAD must be `benchmark` or `smoke`, found {workload}"
        )),
    }
}

fn runner_environment(ghz_version: String) -> Result<GrpcTransportRunner> {
    Ok(GrpcTransportRunner {
        operating_system: operating_system()?,
        kernel_release: command_output(Path::new("uname"), &["-r"])?,
        architecture: std::env::consts::ARCH.to_owned(),
        cpu_model: cpu_model()?,
        logical_cpu_count: std::thread::available_parallelism()
            .context("read logical CPU count")?
            .get(),
        rustc_version: command_output(Path::new("rustc"), &["--version"])?,
        ghz_version,
    })
}

fn required_string(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}

fn required_path(name: &str) -> Result<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{name} is required"))
}

fn required_file(name: &str) -> Result<PathBuf> {
    let path = required_path(name)?;
    ensure!(path.is_file(), "{name} is not a file: {}", path.display());
    Ok(path)
}

fn required_parse<T>(name: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    required_string(name)?
        .parse()
        .map_err(|error| anyhow!("parse {name}: {error}"))
}

fn command_output(program: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .with_context(|| format!("run {} {}", program.display(), arguments.join(" ")))?;
    ensure!(
        output.stdout.len() <= COMMAND_OUTPUT_BYTES_MAX
            && output.stderr.len() <= COMMAND_OUTPUT_BYTES_MAX,
        "{} output exceeded {COMMAND_OUTPUT_BYTES_MAX} bytes",
        program.display()
    );
    ensure!(
        output.status.success(),
        "{} failed with exit {}: {}",
        program.display(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let version_or_output = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    String::from_utf8(version_or_output)
        .with_context(|| format!("{} output is not UTF-8", program.display()))
        .map(|output| output.trim().to_owned())
}

fn operating_system() -> Result<String> {
    let release = fs::read_to_string("/etc/os-release").context("read /etc/os-release")?;
    release
        .lines()
        .find_map(|line| {
            line.strip_prefix("PRETTY_NAME=")
                .map(|value| value.trim_matches('"').to_owned())
        })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("/etc/os-release has no PRETTY_NAME"))
}

fn cpu_model() -> Result<String> {
    let cpu_info = fs::read_to_string("/proc/cpuinfo").context("read /proc/cpuinfo")?;
    cpu_info
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.trim() == "model name")
                .map(|(_, value)| value.trim().to_owned())
        })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("/proc/cpuinfo has no model name"))
}

fn sha256(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn parse_duration(value: &str) -> Result<Duration> {
    let (number, unit) = value
        .find(|character: char| !character.is_ascii_digit())
        .map_or((value, ""), |index| value.split_at(index));
    let number = number
        .parse::<u64>()
        .with_context(|| format!("parse duration {value}"))?;
    let nanoseconds = match unit {
        "ns" => Some(number),
        "us" | "µs" => number.checked_mul(1_000),
        "ms" => number.checked_mul(1_000_000),
        "s" => number.checked_mul(1_000_000_000),
        "m" => number.checked_mul(60_000_000_000),
        _ => None,
    }
    .with_context(|| format!("duration {value} has an unsupported unit or overflows"))?;
    Ok(Duration::from_nanos(nanoseconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_ghz_config_decodes_and_validates() {
        let config = fixed_ghz_config(10_200, 200);

        validate_ghz_config(&config, GhzWorkload::Benchmark).unwrap();
        assert_eq!(
            parse_duration(&config.timeout).unwrap(),
            Duration::from_secs(2)
        );
    }

    #[test]
    fn fixed_smoke_ghz_config_decodes_and_validates() {
        let config = fixed_ghz_config(120, 20);

        validate_ghz_config(&config, GhzWorkload::Smoke).unwrap();
    }

    #[test]
    fn benchmark_workload_rejects_smoke_request_counts() {
        let config = fixed_ghz_config(120, 20);

        assert!(validate_ghz_config(&config, GhzWorkload::Benchmark).is_err());
    }

    #[test]
    fn fixed_ghz_config_rejects_parallel_clients() {
        let mut config = fixed_ghz_config(10_200, 200);
        config.concurrency = 2;

        assert!(validate_ghz_config(&config, GhzWorkload::Benchmark).is_err());
    }

    fn fixed_ghz_config(total: u64, skip_first: u64) -> GhzConfig {
        toml::from_str(&format!(
            r#"
proto = "crates/gtl-wire/proto/gtl/v1/settings.proto"
call = "gtl.v1.SettingsService.GetPushConfirmationRequirement"
total = {total}
skipFirst = {skip_first}
concurrency = 1
connections = 1
cpus = 1
insecure = true
timeout = "2s"
disable-template-data = true
data = {{}}
"#
        ))
        .unwrap()
    }
}
