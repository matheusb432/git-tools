use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use gtl_benchmarks::desktop_scroll::{DesktopScrollRunner, DesktopScrollSystemConditions};

const SYSTEM_FILE_BYTES_MAX: u64 = 256 * 1024;

pub fn describe() -> Result<DesktopScrollRunner> {
    let cpu_information = read_bounded(Path::new("/proc/cpuinfo"))?;
    let cpu_model = cpu_information
        .lines()
        .find_map(|line| line.strip_prefix("model name\t: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("read CPU model from /proc/cpuinfo")?
        .to_owned();
    let logical_cpu_count = cpu_information
        .lines()
        .filter(|line| line.starts_with("processor\t:"))
        .count();
    let operating_system = parse_os_release(&read_bounded(Path::new("/etc/os-release"))?)?;

    Ok(DesktopScrollRunner {
        operating_system,
        kernel_release: read_bounded(Path::new("/proc/sys/kernel/osrelease"))?
            .trim()
            .to_owned(),
        architecture: env::consts::ARCH.to_owned(),
        cpu_model,
        logical_cpu_count,
        rustc_version: required_environment("GTL_DESKTOP_SCROLL_RUSTC_VERSION")?,
        cargo_version: required_environment("GTL_DESKTOP_SCROLL_CARGO_VERSION")?,
        git_version: required_environment("GTL_DESKTOP_SCROLL_GIT_VERSION")?,
        tauri_driver_version: required_environment("GTL_DESKTOP_SCROLL_TAURI_DRIVER_VERSION")?,
        webkitgtk_version: required_environment("GTL_DESKTOP_SCROLL_WEBKITGTK_VERSION")?,
    })
}

pub fn capture_conditions() -> Result<DesktopScrollSystemConditions> {
    let load_average = read_bounded(Path::new("/proc/loadavg"))?;
    let mut fields = load_average.split_whitespace();
    let load_averages = [
        parse_f64(fields.next(), "one-minute load average")?,
        parse_f64(fields.next(), "five-minute load average")?,
        parse_f64(fields.next(), "fifteen-minute load average")?,
    ];
    let memory_information = read_bounded(Path::new("/proc/meminfo"))?;
    let memory_available_kibibytes = memory_information
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemAvailable:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .context("read MemAvailable from /proc/meminfo")?;
    let recorded_at_unix_milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock precedes the Unix epoch")?
        .as_millis()
        .try_into()
        .context("benchmark timestamp exceeds u64 milliseconds")?;

    Ok(DesktopScrollSystemConditions {
        recorded_at_unix_milliseconds,
        load_average_1_minute: load_averages[0],
        load_average_5_minutes: load_averages[1],
        load_average_15_minutes: load_averages[2],
        memory_available_bytes: memory_available_kibibytes
            .checked_mul(1_024)
            .context("convert MemAvailable from KiB to bytes")?,
        cpu_governors: cpu_policy_values("scaling_governor")?,
        energy_performance_preferences: cpu_policy_values("energy_performance_preference")?,
        external_power_online: external_power_online()?,
    })
}

pub fn required_environment(name: &str) -> Result<String> {
    env::var(name).map_err(|_| anyhow!("{name} is required for the desktop scroll benchmark"))
}

pub fn required_environment_path(name: &str) -> Result<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{name} is required for the desktop scroll benchmark"))
}

fn parse_os_release(contents: &str) -> Result<String> {
    let value = contents
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .context("read PRETTY_NAME from /etc/os-release")?;
    Ok(value.trim_matches('"').to_owned())
}

fn parse_f64(value: Option<&str>, label: &str) -> Result<f64> {
    value
        .context(format!("read {label}"))?
        .parse()
        .with_context(|| format!("parse {label}"))
}

fn cpu_policy_values(file_name: &str) -> Result<Vec<String>> {
    let cpu_root = Path::new("/sys/devices/system/cpu");
    let mut values = BTreeSet::new();
    for entry in fs::read_dir(cpu_root).context("read Linux CPU inventory")? {
        let entry = entry.context("read Linux CPU entry")?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.strip_prefix("cpu").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            continue;
        }
        let path = entry.path().join("cpufreq").join(file_name);
        match read_bounded(&path) {
            Ok(value) => {
                values.insert(value.trim().to_owned());
            }
            Err(error) if io_error_kind(&error) == Some(std::io::ErrorKind::NotFound) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(values
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect())
}

fn external_power_online() -> Result<Option<bool>> {
    let root = Path::new("/sys/class/power_supply");
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read Linux power-supply inventory"),
    };
    let mut observed = Vec::new();
    for entry in entries {
        let entry = entry.context("read Linux power-supply entry")?;
        let supply_type = match read_bounded(&entry.path().join("type")) {
            Ok(value) => value,
            Err(error) if io_error_kind(&error) == Some(std::io::ErrorKind::NotFound) => continue,
            Err(error) => return Err(error),
        };
        if !matches!(supply_type.trim(), "Mains" | "USB" | "USB_C") {
            continue;
        }
        let online = read_bounded(&entry.path().join("online"))?;
        observed.push(online.trim() == "1");
    }
    Ok((!observed.is_empty()).then(|| observed.into_iter().any(|online| online)))
}

fn read_bounded(path: &Path) -> Result<String> {
    let metadata =
        fs::metadata(path).with_context(|| format!("read system metadata {}", path.display()))?;
    if metadata.is_file() && metadata.len() > SYSTEM_FILE_BYTES_MAX {
        return Err(anyhow!(
            "system metadata {} exceeds {SYSTEM_FILE_BYTES_MAX} bytes",
            path.display()
        ));
    }
    fs::read_to_string(path).with_context(|| format!("read system metadata {}", path.display()))
}

fn io_error_kind(error: &anyhow::Error) -> Option<std::io::ErrorKind> {
    error.chain().find_map(|source| {
        source
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operating_system_name_is_decoded_without_quotes() {
        assert_eq!(
            parse_os_release("NAME=Example\nPRETTY_NAME=\"Example Linux 1\"\n")
                .expect("parse operating-system name"),
            "Example Linux 1"
        );
    }
}
