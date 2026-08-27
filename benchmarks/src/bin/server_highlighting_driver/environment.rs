use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, ensure};
use gtl_benchmarks::server_highlighting::{ResourceBounds, RunnerEnvironment, SystemConditions};

const COMMAND_OUTPUT_BYTES_MAX: usize = 64 * 1_024;

pub struct DriverConfig {
    pub report_path: PathBuf,
    pub server_binary: PathBuf,
    pub launches: usize,
    pub source_commit: String,
    pub invocation: String,
    pub resource_bounds: ResourceBounds,
    pub rss_sample_interval_microseconds: u64,
    pub massif: Option<MassifConfig>,
}

pub struct MassifConfig {
    pub valgrind_binary: PathBuf,
    pub output_path: PathBuf,
}

impl DriverConfig {
    pub fn from_environment() -> Result<Self> {
        let launches = required_parse("GTL_SERVER_HIGHLIGHTING_LAUNCHES")?;
        ensure!(
            launches > 0,
            "GTL_SERVER_HIGHLIGHTING_LAUNCHES must be positive"
        );
        let source_commit = required_string("GTL_SERVER_HIGHLIGHTING_SOURCE_COMMIT")?;
        ensure!(
            source_commit.len() == 40 && source_commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "GTL_SERVER_HIGHLIGHTING_SOURCE_COMMIT must be a full Git object ID"
        );
        let server_binary = required_path("GTL_SERVER_HIGHLIGHTING_SERVER_BINARY")?;
        ensure!(
            server_binary.is_file(),
            "server highlighting release binary is not a file: {}",
            server_binary.display()
        );
        let massif = match env::var_os("GTL_SERVER_HIGHLIGHTING_MASSIF_OUTPUT") {
            Some(output_path) => Some(MassifConfig {
                valgrind_binary: required_path("GTL_SERVER_HIGHLIGHTING_VALGRIND_BINARY")?,
                output_path: PathBuf::from(output_path),
            }),
            None => None,
        };
        Ok(Self {
            report_path: required_path("GTL_SERVER_HIGHLIGHTING_REPORT_PATH")?,
            server_binary,
            launches,
            source_commit,
            invocation: required_string("GTL_SERVER_HIGHLIGHTING_INVOCATION")?,
            resource_bounds: ResourceBounds {
                cpu_quota_percent: required_parse("GTL_SERVER_HIGHLIGHTING_CPU_QUOTA_PERCENT")?,
                memory_max_bytes: required_parse("GTL_SERVER_HIGHLIGHTING_MEMORY_MAX_BYTES")?,
                memory_swap_max_bytes: required_parse(
                    "GTL_SERVER_HIGHLIGHTING_MEMORY_SWAP_MAX_BYTES",
                )?,
                tasks_max: required_parse("GTL_SERVER_HIGHLIGHTING_TASKS_MAX")?,
                process_niceness: required_parse("GTL_SERVER_HIGHLIGHTING_PROCESS_NICENESS")?,
                cargo_jobs_max: required_parse("GTL_SERVER_HIGHLIGHTING_CARGO_JOBS_MAX")?,
                wall_time_minutes: required_parse("GTL_SERVER_HIGHLIGHTING_WALL_TIME_MINUTES")?,
                termination_grace_seconds: required_parse(
                    "GTL_SERVER_HIGHLIGHTING_TERMINATION_GRACE_SECONDS",
                )?,
            },
            rss_sample_interval_microseconds: required_parse(
                "GTL_SERVER_HIGHLIGHTING_RSS_SAMPLE_INTERVAL_MICROSECONDS",
            )?,
            massif,
        })
    }
}

pub fn runner_environment() -> Result<RunnerEnvironment> {
    Ok(RunnerEnvironment {
        operating_system: operating_system()?,
        kernel_release: command_output("uname", &["-r"])?,
        architecture: std::env::consts::ARCH.to_owned(),
        cpu_model: cpu_model()?,
        logical_cpu_count: std::thread::available_parallelism()
            .context("read logical CPU count")?
            .get(),
        rustc_version: command_output("rustc", &["--version"])?,
        cargo_version: command_output("cargo", &["--version"])?,
        git_version: command_output("git", &["--version"])?,
    })
}

pub fn system_conditions() -> Result<SystemConditions> {
    let load_average = fs::read_to_string("/proc/loadavg").context("read /proc/loadavg")?;
    let loads = parse_load_averages(&load_average)?;
    Ok(SystemConditions {
        recorded_at_unix_milliseconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock precedes Unix epoch")?
            .as_millis()
            .try_into()
            .context("system timestamp exceeds u64 milliseconds")?,
        load_average_1_minute: loads.0,
        load_average_5_minutes: loads.1,
        load_average_15_minutes: loads.2,
        memory_available_bytes: memory_available_bytes()?,
        cpu_governors: read_unique_values("/sys/devices/system/cpu", "cpufreq/scaling_governor")?,
        energy_performance_preferences: read_unique_values(
            "/sys/devices/system/cpu",
            "cpufreq/energy_performance_preference",
        )?,
        external_power_online: external_power_online()?,
    })
}

fn parse_load_averages(value: &str) -> Result<(f64, f64, f64)> {
    let mut fields = value.split_whitespace();
    Ok((
        parse_field(fields.next(), "one-minute load average")?,
        parse_field(fields.next(), "five-minute load average")?,
        parse_field(fields.next(), "fifteen-minute load average")?,
    ))
}

fn required_string(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}

fn required_path(name: &str) -> Result<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{name} is required"))
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

fn command_output(program: &str, arguments: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .with_context(|| format!("run {program} {}", arguments.join(" ")))?;
    ensure!(
        output.stdout.len() <= COMMAND_OUTPUT_BYTES_MAX
            && output.stderr.len() <= COMMAND_OUTPUT_BYTES_MAX,
        "{program} output exceeded {COMMAND_OUTPUT_BYTES_MAX} bytes"
    );
    ensure!(
        output.status.success(),
        "{program} failed with exit {}: {}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout)
        .with_context(|| format!("{program} output is not UTF-8"))
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

fn memory_available_bytes() -> Result<u64> {
    let memory = fs::read_to_string("/proc/meminfo").context("read /proc/meminfo")?;
    let kibibytes = memory
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemAvailable:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .ok_or_else(|| anyhow!("/proc/meminfo has no valid MemAvailable"))?;
    kibibytes
        .checked_mul(1_024)
        .context("MemAvailable overflows bytes")
}

fn read_unique_values(root: &str, suffix: &str) -> Result<Vec<String>> {
    let mut values = BTreeSet::new();
    for entry in fs::read_dir(root).with_context(|| format!("read {root}"))? {
        let entry = entry.with_context(|| format!("read entry in {root}"))?;
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("cpu") {
            continue;
        }
        let path = entry.path().join(suffix);
        match fs::read_to_string(&path) {
            Ok(value) => {
                let value = value.trim();
                if !value.is_empty() {
                    values.insert(value.to_owned());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {}
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        }
    }
    Ok(values.into_iter().collect())
}

fn external_power_online() -> Result<Option<bool>> {
    let root = Path::new("/sys/class/power_supply");
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read power supplies"),
    };
    let mut observed = false;
    let mut online = false;
    for entry in entries {
        let path = entry
            .context("read power-supply entry")?
            .path()
            .join("online");
        match fs::read_to_string(&path) {
            Ok(value) => {
                observed = true;
                online |= value.trim() == "1";
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {}
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        }
    }
    Ok(observed.then_some(online))
}

fn parse_field<T>(value: Option<&str>, label: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .ok_or_else(|| anyhow!("missing {label}"))?
        .parse()
        .map_err(|error| anyhow!("parse {label}: {error}"))
}
