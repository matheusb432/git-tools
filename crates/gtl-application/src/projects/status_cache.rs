use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use gtl_models::{
    paths::RepositoryRoot,
    projects::catalogue::ProjectId,
    repository::status::{RepositoryStatus, StatusHead},
};
use gtl_wire::viewer::projects::ViewerProjectStatus;

const ENTRIES_MAX: usize = 100;
const BYTES_MAX: usize = 128 * 1024;
const LIFETIME: Duration = Duration::from_secs(300);

#[derive(Default)]
pub struct ProjectStatusCache {
    entries: VecDeque<Entry>,
}

struct Entry {
    path: RepositoryRoot,
    status: ViewerProjectStatus,
    touched: Instant,
    bytes: usize,
}

impl ProjectStatusCache {
    pub fn get(
        &mut self,
        id: &ProjectId,
        path: &RepositoryRoot,
        now: Instant,
    ) -> Option<ViewerProjectStatus> {
        self.entries
            .retain(|entry| now.duration_since(entry.touched) < LIFETIME);
        let index = self
            .entries
            .iter()
            .rposition(|entry| entry.status.project_id == *id && entry.path == *path)?;
        let mut entry = self.entries.remove(index)?;
        entry.touched = now;
        let status = entry.status.clone();
        self.entries.push_back(entry);
        Some(status)
    }

    pub fn insert(&mut self, path: RepositoryRoot, status: ViewerProjectStatus, now: Instant) {
        let Ok(bytes) =
            serde_json::to_vec(&status).map(|bytes| bytes.len() + path.as_ref().as_os_str().len())
        else {
            return;
        };
        if bytes > 8192 {
            return;
        }
        self.entries.retain(|entry| {
            now.duration_since(entry.touched) < LIFETIME
                && !(entry.path == path
                    && entry.status.project_id == status.project_id
                    && branch(&entry.status) == branch(&status))
        });
        self.entries.push_back(Entry {
            path,
            status,
            touched: now,
            bytes,
        });
        while self.entries.len() > ENTRIES_MAX
            || self.entries.iter().map(|entry| entry.bytes).sum::<usize>() > BYTES_MAX
        {
            self.entries.pop_front();
        }
    }
}

fn branch(status: &ViewerProjectStatus) -> Option<&str> {
    match &status.status {
        RepositoryStatus::Present {
            head: StatusHead::Branch { name, .. },
            ..
        } => Some(name.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::repository::status::{StatusChanges, StatusUpstream};
    use gtl_wire::viewer::projects::ViewerProjectBranchComparison;

    use super::*;

    fn status(branch: &str) -> anyhow::Result<ViewerProjectStatus> {
        Ok(ViewerProjectStatus {
            project_id: "TST".try_into()?,
            status: RepositoryStatus::Present {
                head: StatusHead::Branch {
                    name: branch.try_into()?,
                    upstream: StatusUpstream::Missing,
                },
                changes: StatusChanges::Clean,
            },
            comparison_branch: gtl_models::projects::comparison::ComparisonBranch::default(),
            branch_comparison: ViewerProjectBranchComparison::Upstream,
        })
    }

    #[test]
    fn latest_branch_value_replaces_history_and_cache_expires() {
        let path =
            RepositoryRoot::try_new("//fixture.invalid/repositories/repos/test".into()).unwrap();
        let now = Instant::now();
        let mut cache = ProjectStatusCache::default();
        for _ in 0..10 {
            cache.insert(path.clone(), status("main").unwrap(), now);
        }
        assert_eq!(cache.entries.len(), 1);
        cache.insert(path.clone(), status("feature").unwrap(), now);
        assert_eq!(cache.entries.len(), 2);
        assert_eq!(
            branch(&cache.get(&"TST".try_into().unwrap(), &path, now).unwrap()),
            Some("feature")
        );
        assert!(
            cache
                .get(&"TST".try_into().unwrap(), &path, now + LIFETIME)
                .is_none()
        );
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn navigation_across_branches_keeps_a_bounded_latest_cache() {
        let path =
            RepositoryRoot::try_new("//fixture.invalid/repositories/repos/test".into()).unwrap();
        let mut cache = ProjectStatusCache::default();
        for index in 0..200 {
            cache.insert(
                path.clone(),
                status(&format!("branch-{index}")).unwrap(),
                Instant::now(),
            );
        }
        assert_eq!(cache.entries.len(), ENTRIES_MAX);
        assert_eq!(
            branch(
                &cache
                    .get(&"TST".try_into().unwrap(), &path, Instant::now())
                    .unwrap()
            ),
            Some("branch-199")
        );
        assert!(cache.entries.iter().map(|entry| entry.bytes).sum::<usize>() <= BYTES_MAX);
    }
}
