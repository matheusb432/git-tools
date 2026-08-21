use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, ensure};
use serde::Serialize;

const PROCESS_COUNT_MAX: usize = 32_768;

#[derive(Debug, Serialize)]
pub struct ProcessMemorySnapshot {
    pub attribution: &'static str,
    pub process_count: usize,
    pub rss_bytes: u64,
}

#[derive(Clone, Copy, Debug)]
struct ProcessIdentity {
    process_id: u32,
    parent_process_id: u32,
}

pub fn snapshot(data_root: &Path) -> Result<ProcessMemorySnapshot> {
    let viewer = required_path("GTL_E2E_VIEWER_BINARY")?
        .canonicalize()
        .context("canonicalize release viewer for RSS attribution")?;
    let identities = process_identities()?;
    let viewer_roots = identities
        .values()
        .filter(|identity| process_matches_viewer(**identity, &viewer, data_root))
        .map(|identity| identity.process_id)
        .collect::<Vec<_>>();
    ensure!(
        viewer_roots.len() == 1,
        "RSS attribution expected one viewer for data root {}, found {}",
        data_root.display(),
        viewer_roots.len()
    );

    let viewer_root = viewer_roots[0];
    let attributed_ids = descendant_processes(viewer_root, &identities);
    let mut rss_bytes = 0_u64;
    let mut measured_process_count = 0_usize;
    for process_id in attributed_ids {
        match process_rss_bytes(process_id) {
            Ok(bytes) => {
                rss_bytes = rss_bytes
                    .checked_add(bytes)
                    .context("sum attributed viewer RSS")?;
                measured_process_count += 1;
            }
            Err(error) if process_id != viewer_root && process_disappeared(&error) => {}
            Err(error) => return Err(error),
        }
    }
    ensure!(
        measured_process_count > 0,
        "RSS attribution found no live viewer processes"
    );

    Ok(ProcessMemorySnapshot {
        attribution: "canonical release viewer executable with the isolated data root, plus descendants",
        process_count: measured_process_count,
        rss_bytes,
    })
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
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => continue,
            Err(error) => return Err(error).context("read Linux process identity"),
        };
        let Some((_, fields)) = stat.rsplit_once(") ") else {
            continue;
        };
        let Some(parent_process_id) = fields
            .split_whitespace()
            .nth(1)
            .and_then(|field| field.parse::<u32>().ok())
        else {
            continue;
        };
        identities.insert(
            process_id,
            ProcessIdentity {
                process_id,
                parent_process_id,
            },
        );
        ensure!(
            identities.len() <= PROCESS_COUNT_MAX,
            "Linux process inventory exceeded {PROCESS_COUNT_MAX} entries"
        );
    }
    Ok(identities)
}

fn process_matches_viewer(identity: ProcessIdentity, viewer: &Path, data_root: &Path) -> bool {
    let process_root = PathBuf::from(format!("/proc/{}", identity.process_id));
    let Ok(executable) = fs::read_link(process_root.join("exe")) else {
        return false;
    };
    if executable != viewer {
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
    viewer_root: u32,
    identities: &BTreeMap<u32, ProcessIdentity>,
) -> BTreeSet<u32> {
    let mut attributed = BTreeSet::from([viewer_root]);
    loop {
        let count_before = attributed.len();
        for identity in identities.values() {
            if attributed.contains(&identity.parent_process_id) {
                attributed.insert(identity.process_id);
            }
        }
        if attributed.len() == count_before {
            return attributed;
        }
    }
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
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descendant_inventory_follows_only_the_viewer_tree() {
        let identities = BTreeMap::from([
            (
                10,
                ProcessIdentity {
                    process_id: 10,
                    parent_process_id: 1,
                },
            ),
            (
                11,
                ProcessIdentity {
                    process_id: 11,
                    parent_process_id: 10,
                },
            ),
            (
                12,
                ProcessIdentity {
                    process_id: 12,
                    parent_process_id: 11,
                },
            ),
            (
                20,
                ProcessIdentity {
                    process_id: 20,
                    parent_process_id: 1,
                },
            ),
        ]);

        assert_eq!(descendant_processes(10, &identities), [10, 11, 12].into());
    }
}
