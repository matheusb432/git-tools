//! Recipe batch identity and managed-repository selection.

use gtl_models::{discovery::DiscoveredRepo, managed::ManagedRepo};

use crate::commands::managed;

/// Mints a fresh batch identifier for recipes opened together.
pub(crate) fn new_batch_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Loads sample_project's active projects and selects repositories with unpushed commits.
pub(crate) fn selected_managed_repos() -> anyhow::Result<Vec<DiscoveredRepo>> {
    let repos = managed::load_projects()?;
    select_managed_repos(repos)
}

fn select_managed_repos(repos: Vec<ManagedRepo>) -> anyhow::Result<Vec<DiscoveredRepo>> {
    Ok(gtl_application::managed::select_unpushed::execute(
        gtl_application::managed::select_unpushed::SelectUnpushed { repos },
        &gtl_infra::git_client::HybridGitClient,
    )?)
}

#[cfg(test)]
mod tests {
    use super::new_batch_id;

    #[test]
    fn new_batch_id_yields_distinct_uuids() {
        assert_ne!(new_batch_id(), new_batch_id());
    }
}
