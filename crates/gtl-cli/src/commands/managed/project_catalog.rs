//! Managed repository catalogue supplied by sample_project.

use anyhow::Context as _;
use gtl_infra::project_repository_client::ProjectRepositoryClient;
use gtl_models::projects::ProjectRepository;

pub fn load_projects() -> anyhow::Result<Vec<ProjectRepository>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the sample_project project client runtime")?;
    runtime
        .block_on(ProjectRepositoryClient::from_environment().list_projects())
        .map_err(anyhow::Error::new)
}
