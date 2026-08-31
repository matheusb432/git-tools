use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, ensure};
use gtl_benchmarks::desktop_scroll::{DesktopScrollProcessMemory, DesktopScrollReadinessSample};

const PROCESS_COUNT_MAX: usize = 32_768;
pub(super) const ATTRIBUTION: &str =
    "canonical release server and viewer executables with the isolated data root, plus descendants";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProcessIdentity {
    process_id: u32,
    parent_process_id: u32,
    cpu_clock_ticks: u64,
}

#[derive(Debug)]
struct AttributedProcessSnapshot {
    memory: DesktopScrollProcessMemory,
    cpu_clock_ticks_by_process: BTreeMap<u32, u64>,
}

#[derive(Debug)]
struct AttributedProcessInventory {
    identities: BTreeMap<u32, ProcessIdentity>,
    viewer_roots: Vec<u32>,
    server_roots: Vec<u32>,
}

#[derive(Default)]
struct AttributedProcessTotals {
    rss_bytes: u64,
    measured_process_count: usize,
    cpu_clock_ticks_by_process: BTreeMap<u32, u64>,
}

impl AttributedProcessTotals {
    fn observe(&mut self, process_id: u32, measurement: Option<(u64, u64)>) -> Result<()> {
        let Some((rss_bytes, cpu_clock_ticks)) = measurement else {
            return Ok(());
        };
        self.rss_bytes = self
            .rss_bytes
            .checked_add(rss_bytes)
            .context("sum attributed application RSS")?;
        self.measured_process_count += 1;
        self.cpu_clock_ticks_by_process
            .insert(process_id, cpu_clock_ticks);
        Ok(())
    }
}

pub struct ReadinessProcessSampler {
    data_root: PathBuf,
    cpu_clock_ticks_by_process_started: BTreeMap<u32, u64>,
    cpu_clock_ticks_by_process_latest: BTreeMap<u32, u64>,
    peak_memory: DesktopScrollProcessMemory,
}

impl ReadinessProcessSampler {
    pub fn try_start(data_root: &Path) -> Result<Option<Self>> {
        let inventory = attributed_process_inventory(data_root)?;
        if inventory.viewer_roots.len() != 1 || inventory.server_roots.len() != 1 {
            return Ok(None);
        }
        let snapshot = inventory.into_snapshot()?;
        Ok(Some(Self {
            data_root: data_root.to_path_buf(),
            cpu_clock_ticks_by_process_started: snapshot.cpu_clock_ticks_by_process.clone(),
            cpu_clock_ticks_by_process_latest: snapshot.cpu_clock_ticks_by_process,
            peak_memory: snapshot.memory,
        }))
    }

    pub fn observe(&mut self) -> Result<()> {
        let snapshot = readiness_attributed_process_snapshot(&self.data_root)?;
        if snapshot.memory.rss_bytes > self.peak_memory.rss_bytes {
            self.peak_memory = snapshot.memory;
        }
        for (process_id, cpu_clock_ticks) in snapshot.cpu_clock_ticks_by_process {
            self.cpu_clock_ticks_by_process_latest
                .insert(process_id, cpu_clock_ticks);
        }
        Ok(())
    }

    pub fn finish(mut self, wall_time: Duration) -> Result<DesktopScrollReadinessSample> {
        self.observe()?;
        Ok(DesktopScrollReadinessSample {
            wall_time_milliseconds: wall_time
                .as_millis()
                .try_into()
                .context("readiness wall time exceeds u64 milliseconds")?,
            process_cpu_clock_ticks: process_cpu_clock_ticks_delta(
                &self.cpu_clock_ticks_by_process_started,
                &self.cpu_clock_ticks_by_process_latest,
            )?,
            peak_memory: self.peak_memory,
        })
    }
}

pub fn clock_ticks_per_second() -> Result<u64> {
    // sysconf reads a process-wide constant and does not dereference caller-owned memory.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    ensure!(ticks > 0, "read positive process CPU clock frequency");
    u64::try_from(ticks).context("process CPU clock frequency exceeds u64")
}

pub fn snapshot(data_root: &Path) -> Result<DesktopScrollProcessMemory> {
    attributed_process_snapshot(data_root).map(|snapshot| snapshot.memory)
}

fn attributed_process_snapshot(data_root: &Path) -> Result<AttributedProcessSnapshot> {
    let inventory = attributed_process_inventory(data_root)?;
    ensure!(
        inventory.viewer_roots.len() == 1 && inventory.server_roots.len() == 1,
        "RSS attribution expected one top-level viewer and server for data root {}, found {} viewer and {} server roots",
        data_root.display(),
        inventory.viewer_roots.len(),
        inventory.server_roots.len()
    );
    inventory.into_snapshot()
}

fn readiness_attributed_process_snapshot(data_root: &Path) -> Result<AttributedProcessSnapshot> {
    let inventory = attributed_process_inventory(data_root)?;
    ensure!(
        !inventory.viewer_roots.is_empty() && !inventory.server_roots.is_empty(),
        "readiness attribution lost the viewer or server process tree"
    );
    inventory.into_snapshot()
}

fn attributed_process_inventory(data_root: &Path) -> Result<AttributedProcessInventory> {
    let viewer = required_path("GTL_E2E_VIEWER_BINARY")?
        .canonicalize()
        .context("canonicalize release viewer for RSS attribution")?;
    let server = required_path("GTL_E2E_SERVER_BINARY")?
        .canonicalize()
        .context("canonicalize release server for RSS attribution")?;
    let identities = process_identities()?;
    let viewer_processes = identities
        .values()
        .filter(|identity| process_matches_executable(**identity, &viewer, data_root))
        .map(|identity| identity.process_id)
        .collect::<BTreeSet<_>>();
    let viewer_roots = outermost_processes(&viewer_processes, &identities);
    let server_processes = identities
        .values()
        .filter(|identity| process_matches_executable(**identity, &server, data_root))
        .map(|identity| identity.process_id)
        .collect::<BTreeSet<_>>();
    let server_roots = outermost_processes(&server_processes, &identities);

    Ok(AttributedProcessInventory {
        identities,
        viewer_roots,
        server_roots,
    })
}

impl AttributedProcessInventory {
    fn into_snapshot(self) -> Result<AttributedProcessSnapshot> {
        let mut roots = self.viewer_roots;
        roots.extend(self.server_roots);
        let attributed_ids = descendant_processes(&roots, &self.identities);
        let mut totals = AttributedProcessTotals::default();
        for process_id in attributed_ids {
            let measurement = measure_attributed_process(process_id, &roots, &self.identities)?;
            totals.observe(process_id, measurement)?;
        }
        ensure!(
            totals.measured_process_count >= roots.len(),
            "RSS attribution lost a required application process"
        );

        Ok(AttributedProcessSnapshot {
            memory: DesktopScrollProcessMemory {
                attribution: ATTRIBUTION.to_owned(),
                process_count: totals.measured_process_count,
                rss_bytes: totals.rss_bytes,
            },
            cpu_clock_ticks_by_process: totals.cpu_clock_ticks_by_process,
        })
    }
}

fn measure_attributed_process(
    process_id: u32,
    roots: &[u32],
    identities: &BTreeMap<u32, ProcessIdentity>,
) -> Result<Option<(u64, u64)>> {
    match process_rss_bytes(process_id) {
        Ok(rss_bytes) => Ok(Some((rss_bytes, identities[&process_id].cpu_clock_ticks))),
        Err(error) if !roots.contains(&process_id) && process_disappeared(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn required_path(name: &str) -> Result<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{name} is required for RSS attribution"))
}

fn process_identities() -> Result<BTreeMap<u32, ProcessIdentity>> {
    let mut identities = BTreeMap::new();
    for entry in fs::read_dir("/proc").context("read Linux process table")? {
        let entry = entry.context("read Linux process-table entry")?;
        let Some(process_id) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let stat = match fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if process_identity_is_unavailable(&error) => continue,
            Err(error) => return Err(error).context("read Linux process identity"),
        };
        let Some(identity) = parse_process_identity(process_id, &stat) else {
            continue;
        };
        identities.insert(process_id, identity);
        ensure!(
            identities.len() <= PROCESS_COUNT_MAX,
            "Linux process inventory exceeded {PROCESS_COUNT_MAX} entries"
        );
    }
    Ok(identities)
}

fn parse_process_identity(process_id: u32, stat: &str) -> Option<ProcessIdentity> {
    let (_, fields) = stat.rsplit_once(") ")?;
    let fields = fields.split_whitespace().collect::<Vec<_>>();
    let parent_process_id = fields.get(1)?.parse().ok()?;
    let user_cpu_clock_ticks = fields.get(11)?.parse::<u64>().ok()?;
    let system_cpu_clock_ticks = fields.get(12)?.parse::<u64>().ok()?;
    let cpu_clock_ticks = user_cpu_clock_ticks.checked_add(system_cpu_clock_ticks)?;
    Some(ProcessIdentity {
        process_id,
        parent_process_id,
        cpu_clock_ticks,
    })
}

fn process_cpu_clock_ticks_delta(
    started: &BTreeMap<u32, u64>,
    latest: &BTreeMap<u32, u64>,
) -> Result<u64> {
    latest
        .iter()
        .try_fold(0_u64, |total, (process_id, latest_ticks)| {
            let started_ticks = started.get(process_id).copied().unwrap_or(0);
            let delta = latest_ticks.checked_sub(started_ticks).with_context(|| {
                format!("attributed process {process_id} CPU clock moved backwards")
            })?;
            total
                .checked_add(delta)
                .context("sum attributed process CPU clock ticks")
        })
}

fn process_matches_executable(
    identity: ProcessIdentity,
    executable: &Path,
    data_root: &Path,
) -> bool {
    let process_root = PathBuf::from(format!("/proc/{}", identity.process_id));
    let Ok(process_executable) = fs::read_link(process_root.join("exe")) else {
        return false;
    };
    if process_executable != executable {
        return false;
    }
    let Ok(environment) = fs::read(process_root.join("environ")) else {
        return false;
    };
    let mut expected = b"GIT_TOOLS_DATA_DIR=".to_vec();
    expected.extend_from_slice(data_root.as_os_str().as_bytes());
    environment
        .split(|byte| *byte == 0)
        .any(|variable| variable == expected)
}

fn descendant_processes(
    roots: &[u32],
    identities: &BTreeMap<u32, ProcessIdentity>,
) -> BTreeSet<u32> {
    let mut attributed = roots.iter().copied().collect::<BTreeSet<_>>();
    loop {
        let count_before = attributed.len();
        let descendants = identities
            .values()
            .filter(|identity| attributed.contains(&identity.parent_process_id))
            .map(|identity| identity.process_id)
            .collect::<Vec<_>>();
        attributed.extend(descendants);
        if attributed.len() == count_before {
            return attributed;
        }
    }
}

fn outermost_processes(
    processes: &BTreeSet<u32>,
    identities: &BTreeMap<u32, ProcessIdentity>,
) -> Vec<u32> {
    processes
        .iter()
        .copied()
        .filter(|process_id| is_outermost_process(*process_id, processes, identities))
        .collect()
}

fn is_outermost_process(
    process_id: u32,
    processes: &BTreeSet<u32>,
    identities: &BTreeMap<u32, ProcessIdentity>,
) -> bool {
    let mut visited = BTreeSet::new();
    let mut ancestor = identities
        .get(&process_id)
        .map(|identity| identity.parent_process_id);
    while let Some(ancestor_id) = ancestor {
        if processes.contains(&ancestor_id) {
            return false;
        }
        if !visited.insert(ancestor_id) {
            return true;
        }
        ancestor = identities
            .get(&ancestor_id)
            .map(|identity| identity.parent_process_id);
    }
    true
}

fn process_rss_bytes(process_id: u32) -> Result<u64> {
    let path = PathBuf::from(format!("/proc/{process_id}/status"));
    let status = fs::read_to_string(&path)
        .with_context(|| format!("read attributed process status {}", path.display()))?;
    let kibibytes = status
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .with_context(|| format!("read VmRSS from {}", path.display()))?;
    kibibytes
        .checked_mul(1_024)
        .context("convert attributed RSS from KiB to bytes")
}

fn process_disappeared(error: &anyhow::Error) -> bool {
    error.chain().any(|source| {
        source
            .downcast_ref::<std::io::Error>()
            .is_some_and(process_disappeared_from_table)
    })
}

fn process_identity_is_unavailable(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied || process_disappeared_from_table(error)
}

fn process_disappeared_from_table(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descendant_inventory_unites_application_trees_without_double_counting() {
        let identities = BTreeMap::from([
            (
                10,
                ProcessIdentity {
                    process_id: 10,
                    parent_process_id: 1,
                    cpu_clock_ticks: 0,
                },
            ),
            (
                11,
                ProcessIdentity {
                    process_id: 11,
                    parent_process_id: 10,
                    cpu_clock_ticks: 0,
                },
            ),
            (
                12,
                ProcessIdentity {
                    process_id: 12,
                    parent_process_id: 11,
                    cpu_clock_ticks: 0,
                },
            ),
            (
                20,
                ProcessIdentity {
                    process_id: 20,
                    parent_process_id: 10,
                    cpu_clock_ticks: 0,
                },
            ),
            (
                30,
                ProcessIdentity {
                    process_id: 30,
                    parent_process_id: 1,
                    cpu_clock_ticks: 0,
                },
            ),
        ]);

        assert_eq!(
            descendant_processes(&[10, 20], &identities),
            [10, 11, 12, 20].into()
        );
    }

    #[test]
    fn outermost_inventory_collapses_matching_descendants() {
        let identities = BTreeMap::from([
            (
                10,
                ProcessIdentity {
                    process_id: 10,
                    parent_process_id: 1,
                    cpu_clock_ticks: 0,
                },
            ),
            (
                11,
                ProcessIdentity {
                    process_id: 11,
                    parent_process_id: 10,
                    cpu_clock_ticks: 0,
                },
            ),
            (
                12,
                ProcessIdentity {
                    process_id: 12,
                    parent_process_id: 11,
                    cpu_clock_ticks: 0,
                },
            ),
            (
                20,
                ProcessIdentity {
                    process_id: 20,
                    parent_process_id: 1,
                    cpu_clock_ticks: 0,
                },
            ),
        ]);

        assert_eq!(
            outermost_processes(&BTreeSet::from([10, 11, 12, 20]), &identities),
            vec![10, 20]
        );
    }

    #[test]
    fn process_stat_parser_handles_spaces_and_parentheses_in_command_name() {
        let stat = "42 (viewer worker (web)) S 7 0 0 0 0 0 0 0 0 0 13 17";

        assert_eq!(
            parse_process_identity(42, stat),
            Some(ProcessIdentity {
                process_id: 42,
                parent_process_id: 7,
                cpu_clock_ticks: 30,
            })
        );
    }

    #[test]
    fn cpu_delta_includes_new_processes_and_existing_process_growth() {
        let started = BTreeMap::from([(10, 100), (20, 50)]);
        let latest = BTreeMap::from([(10, 130), (20, 55), (30, 7)]);

        assert_eq!(
            process_cpu_clock_ticks_delta(&started, &latest).unwrap(),
            42
        );
    }

    #[test]
    fn process_inventory_treats_a_racing_linux_process_as_unavailable() {
        let error = std::io::Error::from_raw_os_error(libc::ESRCH);

        assert!(process_identity_is_unavailable(&error));
        assert!(process_disappeared_from_table(&error));
    }
}
