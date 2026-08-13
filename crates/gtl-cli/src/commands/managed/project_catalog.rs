//! Managed repository catalogue supplied by sample_project.

use anyhow::Context as _;
use gtl_infra::managed_repo_client::ManagedRepoClient;
use gtl_models::managed::ManagedRepo;

pub fn load_projects() -> anyhow::Result<Vec<ManagedRepo>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the sample_project project client runtime")?;
    runtime
        .block_on(ManagedRepoClient::from_environment().list_projects())
        .map_err(anyhow::Error::new)
}
