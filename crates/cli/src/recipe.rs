//! Recipe batch identity and managed-repository selection.

use domain::{discovery::DiscoveredRepo, managed::ManagedRepo};

use crate::commands::managed::{self, ManagedOptions};

/// Mints a fresh batch identifier for recipes opened together.
pub(crate) fn new_batch_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Loads the managed manifest and selects repositories with unpushed commits.
pub(crate) fn selected_managed_repos(
    options: &ManagedOptions,
) -> anyhow::Result<Vec<DiscoveredRepo>> {
    let repos = managed::load_repos(options)?;
    select_managed_repos(repos)
}

fn select_managed_repos(repos: Vec<ManagedRepo>) -> anyhow::Result<Vec<DiscoveredRepo>> {
    Ok(application::managed::select_unpushed::execute(
        application::managed::select_unpushed::SelectUnpushed { repos },
        &infra::git_client::HybridGitClient,
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
