use std::future::Future;

use gtl_models::managed::ManagedRepo;

#[derive(Debug, thiserror::Error)]
pub enum ProjectClientError {
    #[error("project catalogue is unavailable: {message}")]
    Unavailable { message: String },
    #[error("project catalogue returned invalid data: {message}")]
    InvalidData { message: String },
}

pub trait ProjectClient: Clone + Send + Sync + 'static {
    fn list_projects(
        &self,
    ) -> impl Future<Output = Result<Vec<ManagedRepo>, ProjectClientError>> + Send;
}
