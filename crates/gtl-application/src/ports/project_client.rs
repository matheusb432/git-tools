use std::future::Future;

use gtl_models::projects::ProjectRepository;

#[derive(Debug, thiserror::Error)]
pub enum ProjectClientError {
    #[error("project catalogue is unavailable: {message}")]
    Unavailable {
        message: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("project catalogue returned invalid data: {message}")]
    InvalidData {
        message: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

pub trait ProjectClient: Clone + Send + Sync + 'static {
    fn list_projects(
        &self,
    ) -> impl Future<Output = Result<Vec<ProjectRepository>, ProjectClientError>> + Send;
}
