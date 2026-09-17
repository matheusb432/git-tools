use std::collections::VecDeque;

use dioxus::prelude::*;
use gtl_wire::viewer::projects::{ViewerProject, ViewerProjectStatus, ViewerProjectStatusUpdate};

#[derive(Clone, Default, PartialEq)]
pub(super) struct ProjectStatusLoad {
    pub(super) status: Option<ViewerProjectStatus>,
    pub(super) failed: bool,
}

#[derive(Default)]
pub(crate) struct ProjectStatusCache {
    instance: Option<String>,
    entries: VecDeque<Entry>,
    retained: Vec<ViewerProject>,
}

struct Entry {
    project: ViewerProject,
    load: ProjectStatusLoad,
    touched_ms: f64,
    bytes: usize,
}

impl ProjectStatusCache {
    pub(super) fn retain_page(&mut self, instance: Option<String>, projects: &[ViewerProject]) {
        let now = now_ms();
        for index in self.retained_indices() {
            self.entries[index].touched_ms = now;
        }
        if self.instance != instance {
            self.entries.clear();
            self.instance = instance;
        }
        self.entries
            .retain(|entry| now - entry.touched_ms < 300_000.0);
        self.retained = projects.to_vec();
        self.trim();
    }

    pub(super) fn release_page(&mut self) {
        let now = now_ms();
        for index in self.retained_indices() {
            self.entries[index].touched_ms = now;
        }
        self.retained.clear();
        self.trim();
    }

    fn latest_index(&self, project: &ViewerProject) -> Option<usize> {
        self.entries
            .iter()
            .rposition(|entry| entry.project.id == project.id && entry.project.path == project.path)
    }

    fn retained_indices(&self) -> Vec<usize> {
        self.retained
            .iter()
            .filter_map(|project| self.latest_index(project))
            .collect()
    }

    fn eviction(&self) -> Option<usize> {
        let protected = self.retained_indices();
        let inactive = self
            .entries
            .iter()
            .enumerate()
            .filter(|(index, _)| !protected.contains(index))
            .collect::<Vec<_>>();
        if inactive.len() <= 100
            && inactive.iter().map(|(_, entry)| entry.bytes).sum::<usize>() <= 128 * 1024
        {
            return None;
        }
        inactive.first().map(|(index, _)| *index)
    }

    fn trim(&mut self) {
        while let Some(index) = self.eviction() {
            self.entries.remove(index);
        }
    }

    pub(super) fn get(&self, project: &ViewerProject, instance: Option<&str>) -> ProjectStatusLoad {
        if self.instance.as_deref() != instance {
            return ProjectStatusLoad::default();
        }
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.project.id == project.id && entry.project.path == project.path)
            .map_or_else(ProjectStatusLoad::default, |entry| entry.load.clone())
    }

    pub(super) fn needs_update(
        &self,
        projects: &[ViewerProject],
        update: &ViewerProjectStatusUpdate,
    ) -> bool {
        let id = match update {
            ViewerProjectStatusUpdate::Status(status) => &status.project_id,
            ViewerProjectStatusUpdate::Unavailable(id) => id,
        };
        let Some(project) = projects.iter().find(|project| project.id == *id) else {
            return false;
        };
        let Some(entry) = self.latest_index(project).map(|index| &self.entries[index]) else {
            return true;
        };
        match update {
            ViewerProjectStatusUpdate::Status(status) => {
                entry.load.failed || entry.load.status.as_ref() != Some(status)
            }
            ViewerProjectStatusUpdate::Unavailable(_) => !entry.load.failed,
        }
    }

    pub(super) fn observe(
        &mut self,
        projects: &[ViewerProject],
        update: ViewerProjectStatusUpdate,
    ) {
        let id = match &update {
            ViewerProjectStatusUpdate::Status(status) => &status.project_id,
            ViewerProjectStatusUpdate::Unavailable(id) => id,
        };
        let Some(project) = projects.iter().find(|project| project.id == *id) else {
            return;
        };
        let mut load = self.get(project, self.instance.as_deref());
        match update {
            ViewerProjectStatusUpdate::Status(status) => {
                load = ProjectStatusLoad {
                    status: Some(status),
                    failed: false,
                };
            }
            ViewerProjectStatusUpdate::Unavailable(_) => {
                load.failed = true;
            }
        }
        let mut bytes = serde_json::to_vec(&load.status).map_or(8193, |bytes| bytes.len())
            + project.path.as_ref().as_os_str().len();
        if bytes > 8192 {
            load = self.get(project, self.instance.as_deref());
            load.failed = true;
            bytes = serde_json::to_vec(&load.status).map_or(0, |bytes| bytes.len())
                + project.path.as_ref().as_os_str().len();
        }
        self.entries.retain(|entry| {
            entry.project.id != project.id
                || entry.project.path != project.path
                || branch(&entry.load) != branch(&load)
        });
        self.entries.push_back(Entry {
            project: project.clone(),
            load,
            touched_ms: now_ms(),
            bytes,
        });
        self.trim();
    }
}

fn branch(load: &ProjectStatusLoad) -> Option<&str> {
    use gtl_models::repository::status::{RepositoryStatus, StatusHead};
    match load.status.as_ref().map(|status| &status.status) {
        Some(RepositoryStatus::Present {
            head: StatusHead::Branch { name, .. },
            ..
        }) => Some(name.as_str()),
        _ => None,
    }
}

fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}

pub(crate) fn use_status_cache_provider() {
    let cache = use_signal(ProjectStatusCache::default);
    use_context_provider(|| cache);
}

#[cfg(test)]
mod tests {
    use gtl_models::repository::status::RepositoryStatus;
    use gtl_wire::viewer::projects::ViewerProjectBranchComparison;

    use super::*;

    #[test]
    fn refresh_failure_retains_values_and_server_replacement_clears_them() {
        let project = ViewerProject {
            id: "TST".try_into().unwrap(),
            name: "Test".try_into().unwrap(),
            path: gtl_models::paths::RepositoryRoot::try_new("/repos/test".into()).unwrap(),
            comparison_branch: gtl_models::projects::comparison::ComparisonBranch::default(),
            last_rendered_at: None,
        };
        let status = ViewerProjectStatus {
            project_id: project.id.clone(),
            status: RepositoryStatus::Absent,
            comparison_branch: gtl_models::projects::comparison::ComparisonBranch::default(),
            branch_comparison: ViewerProjectBranchComparison::Upstream,
        };
        let projects = [project.clone()];
        let mut cache = ProjectStatusCache::default();
        cache.retain_page(Some("first".into()), &projects);
        cache.observe(&projects, ViewerProjectStatusUpdate::Status(status.clone()));
        assert!(!cache.needs_update(
            &projects,
            &ViewerProjectStatusUpdate::Status(status.clone())
        ));
        cache.observe(
            &projects,
            ViewerProjectStatusUpdate::Unavailable(project.id.clone()),
        );
        let failed = cache.get(&project, Some("first"));
        assert_eq!(failed.status, Some(status.clone()));
        assert!(failed.failed);
        cache.entries[0].touched_ms = -400_000.0;
        cache.retain_page(Some("first".into()), &projects);
        assert!(cache.get(&project, Some("first")).status.is_some());
        cache.observe(&projects, ViewerProjectStatusUpdate::Status(status));
        assert!(!cache.get(&project, Some("first")).failed);
        assert_eq!(cache.entries.len(), 1);
        cache.retain_page(Some("replacement".into()), &projects);
        assert!(cache.get(&project, Some("replacement")).status.is_none());
        assert!(cache.entries.is_empty());
    }
    #[test]
    fn branch_churn_cannot_evict_an_unchanged_visible_project() {
        use gtl_models::repository::status::{StatusChanges, StatusHead, StatusUpstream};
        let project = ViewerProject {
            id: "TST".try_into().unwrap(),
            name: "Test".try_into().unwrap(),
            path: gtl_models::paths::RepositoryRoot::try_new("/repos/test".into()).unwrap(),
            comparison_branch: gtl_models::projects::comparison::ComparisonBranch::default(),
            last_rendered_at: None,
        };
        let quiet = ViewerProject {
            id: "QUI".try_into().unwrap(),
            ..project.clone()
        };
        let projects = [project.clone(), quiet.clone()];
        let mut cache = ProjectStatusCache::default();
        cache.retain_page(Some("server".into()), &projects);
        cache.observe(
            &projects,
            ViewerProjectStatusUpdate::Status(ViewerProjectStatus {
                project_id: quiet.id.clone(),
                status: RepositoryStatus::Absent,
                comparison_branch: project.comparison_branch.clone(),
                branch_comparison: ViewerProjectBranchComparison::Upstream,
            }),
        );
        for index in 0..130 {
            cache.observe(
                &projects,
                ViewerProjectStatusUpdate::Status(ViewerProjectStatus {
                    project_id: project.id.clone(),
                    status: RepositoryStatus::Present {
                        head: StatusHead::Branch {
                            name: format!("branch-{index}").try_into().unwrap(),
                            upstream: StatusUpstream::Missing,
                        },
                        changes: StatusChanges::Clean,
                    },
                    comparison_branch: project.comparison_branch.clone(),
                    branch_comparison: ViewerProjectBranchComparison::Upstream,
                }),
            );
        }
        assert!(cache.get(&quiet, Some("server")).status.is_some());
        assert_eq!(cache.entries.len(), 102);
        cache.entries[1].touched_ms = -400_000.0;
        cache.retain_page(Some("server".into()), &projects);
        assert_eq!(cache.entries.len(), 101);
        assert!(cache.get(&quiet, Some("server")).status.is_some());
        cache.release_page();
        assert_eq!(cache.entries.len(), 100);
    }
}
