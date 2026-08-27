use std::{fs, io, path::PathBuf};

use super::ProcessMemory;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessSample {
    pub cpu_time_nanoseconds: u64,
    pub cpu_clock_ticks: u64,
    pub memory: ProcessMemory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessRss {
    pub current: u64,
    pub high_water: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessSampleError {
    #[error("read Linux process measurement `{path}`")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Linux process measurement is malformed: {reason}")]
    Malformed { reason: String },
}

pub fn clock_ticks_per_second() -> Result<u64, ProcessSampleError> {
    // sysconf reads a process-wide constant and does not dereference caller-owned memory.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    u64::try_from(ticks).map_err(|_| ProcessSampleError::Malformed {
        reason: "process CPU clock frequency is not positive".to_owned(),
    })
}

pub fn read_process_sample(process_id: u32) -> Result<ProcessSample, ProcessSampleError> {
    let process_root = PathBuf::from(format!("/proc/{process_id}"));
    let stat_path = process_root.join("stat");
    let status_path = process_root.join("status");
    let smaps_path = process_root.join("smaps_rollup");
    let stat = fs::read_to_string(&stat_path).map_err(|source| ProcessSampleError::Read {
        path: stat_path,
        source,
    })?;
    let status = fs::read_to_string(&status_path).map_err(|source| ProcessSampleError::Read {
        path: status_path,
        source,
    })?;
    let smaps = fs::read_to_string(&smaps_path).map_err(|source| ProcessSampleError::Read {
        path: smaps_path,
        source,
    })?;
    let cpu_time_nanoseconds = read_process_cpu_time_nanoseconds(process_id)?;
    parse_process_sample(&stat, &status, &smaps, cpu_time_nanoseconds)
}

pub fn read_process_rss(process_id: u32) -> Result<ProcessRss, ProcessSampleError> {
    let path = PathBuf::from(format!("/proc/{process_id}/status"));
    let status =
        fs::read_to_string(&path).map_err(|source| ProcessSampleError::Read { path, source })?;
    parse_process_rss(&status)
}

pub fn parse_process_rss(status: &str) -> Result<ProcessRss, ProcessSampleError> {
    Ok(ProcessRss {
        current: parse_kibibytes(status, "VmRSS:")?,
        high_water: parse_kibibytes(status, "VmHWM:")?,
    })
}

pub fn parse_process_sample(
    stat: &str,
    status: &str,
    smaps_rollup: &str,
    cpu_time_nanoseconds: u64,
) -> Result<ProcessSample, ProcessSampleError> {
    let (_, stat_fields) = stat
        .rsplit_once(") ")
        .ok_or_else(|| malformed("process stat has no command terminator"))?;
    let fields = stat_fields.split_whitespace().collect::<Vec<_>>();
    let user_ticks = parse_stat_field(&fields, 11, "user CPU clock ticks")?;
    let system_ticks = parse_stat_field(&fields, 12, "system CPU clock ticks")?;
    let cpu_clock_ticks = user_ticks
        .checked_add(system_ticks)
        .ok_or_else(|| malformed("process CPU clock tick sum overflows"))?;
    Ok(ProcessSample {
        cpu_time_nanoseconds,
        cpu_clock_ticks,
        memory: ProcessMemory {
            rss: parse_kibibytes(status, "VmRSS:")?,
            high_water: parse_kibibytes(status, "VmHWM:")?,
            anonymous: parse_kibibytes(smaps_rollup, "Anonymous:")?,
            private_dirty: parse_kibibytes(smaps_rollup, "Private_Dirty:")?,
        },
    })
}

fn read_process_cpu_time_nanoseconds(process_id: u32) -> Result<u64, ProcessSampleError> {
    const THREAD_COUNT_MAX: usize = 1_024;
    let task_root = PathBuf::from(format!("/proc/{process_id}/task"));
    let entries = fs::read_dir(&task_root).map_err(|source| ProcessSampleError::Read {
        path: task_root.clone(),
        source,
    })?;
    let mut total = 0_u64;
    let mut thread_count = 0_usize;
    for entry in entries {
        let entry = entry.map_err(|source| ProcessSampleError::Read {
            path: task_root.clone(),
            source,
        })?;
        if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let path = entry.path().join("schedstat");
        let schedstat = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => return Err(ProcessSampleError::Read { path, source }),
        };
        let runtime = parse_schedstat_cpu_time(&schedstat)?;
        total = total
            .checked_add(runtime)
            .ok_or_else(|| malformed("sum of server thread CPU runtimes overflows"))?;
        thread_count += 1;
        if thread_count > THREAD_COUNT_MAX {
            return Err(malformed(format!(
                "server process exceeds {THREAD_COUNT_MAX} threads"
            )));
        }
    }
    if thread_count == 0 {
        return Err(malformed("server process has no measurable threads"));
    }
    Ok(total)
}

fn parse_schedstat_cpu_time(schedstat: &str) -> Result<u64, ProcessSampleError> {
    schedstat
        .split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| malformed("process schedstat has no valid CPU runtime"))
}

fn parse_stat_field(fields: &[&str], index: usize, label: &str) -> Result<u64, ProcessSampleError> {
    fields
        .get(index)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| malformed(format!("process stat has no valid {label}")))
}

fn parse_kibibytes(text: &str, label: &str) -> Result<u64, ProcessSampleError> {
    let kibibytes = text
        .lines()
        .find_map(|line| {
            line.strip_prefix(label)
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .ok_or_else(|| malformed(format!("process measurement has no valid {label}")))?;
    kibibytes
        .checked_mul(1_024)
        .ok_or_else(|| malformed(format!("process measurement {label} overflows bytes")))
}

fn malformed(reason: impl Into<String>) -> ProcessSampleError {
    ProcessSampleError::Malformed {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_handles_parentheses_in_command_and_all_memory_fields() {
        let stat = "42 (server worker (rows)) S 7 0 0 0 0 0 0 0 0 0 13 17";
        let status = "Name:\tgtl-server\nVmHWM:\t  120 kB\nVmRSS:\t  100 kB\n";
        let smaps = "Rss: 100 kB\nPrivate_Dirty: 40 kB\nAnonymous: 60 kB\n";

        assert_eq!(
            parse_process_sample(stat, status, smaps, 123_456).unwrap(),
            ProcessSample {
                cpu_time_nanoseconds: 123_456,
                cpu_clock_ticks: 30,
                memory: ProcessMemory {
                    rss: 100 * 1_024,
                    high_water: 120 * 1_024,
                    anonymous: 60 * 1_024,
                    private_dirty: 40 * 1_024,
                },
            }
        );
    }

    #[test]
    fn parser_rejects_missing_required_measurements() {
        let error = parse_process_sample("invalid", "", "", 0).unwrap_err();

        assert!(error.to_string().contains("command terminator"));
    }

    #[test]
    fn rss_parser_reads_current_and_process_high_water() {
        assert_eq!(
            parse_process_rss("VmHWM:\t120 kB\nVmRSS:\t100 kB\n").unwrap(),
            ProcessRss {
                current: 100 * 1_024,
                high_water: 120 * 1_024,
            }
        );
    }

    #[test]
    fn schedstat_parser_reads_nanosecond_runtime() {
        assert_eq!(parse_schedstat_cpu_time("123456 789 10").unwrap(), 123_456);
        assert!(parse_schedstat_cpu_time("").is_err());
    }
}
