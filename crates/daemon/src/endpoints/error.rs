use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use super::error_envelope;

#[derive(Debug, thiserror::Error)]
pub(crate) enum EndpointError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0:#}")]
    Unexpected(#[source] anyhow::Error),
    #[error("daemon task panicked: {0}")]
    TaskJoin(#[source] tokio::task::JoinError),
}

impl EndpointError {
    pub(crate) fn bad_request(error: impl std::fmt::Display) -> Self {
        Self::BadRequest(error.to_string())
    }

    pub(crate) fn unexpected(error: impl Into<anyhow::Error>) -> Self {
        Self::Unexpected(error.into())
    }

    pub(crate) fn task_join(error: tokio::task::JoinError) -> Self {
        Self::TaskJoin(error)
    }
}

impl IntoResponse for EndpointError {
    fn into_response(self) -> Response {
        let status = if matches!(&self, Self::BadRequest(_)) {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, Json(error_envelope::<()>(self.to_string()))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::EndpointError;

    #[test]
    fn unexpected_error_retains_its_source() {
        let error = anyhow::anyhow!("artifact storage failed");

        let endpoint = EndpointError::unexpected(error);

        assert!(endpoint.source().is_some());
    }
}
