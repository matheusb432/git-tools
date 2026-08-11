use gtl_contracts::viewer::{LoadViewerDiffLines, ViewerDiffLines};

use super::super::api::DiffViewerApi;
use crate::shared::bridge::ClientApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientDiffSource {
    Desktop,
}

impl ClientDiffSource {
    pub(super) async fn load_diff_lines(
        self,
        request: LoadViewerDiffLines,
    ) -> Result<ViewerDiffLines, ClientDiffSourceError> {
        match self {
            Self::Desktop => DiffViewerApi::load_diff_lines(request)
                .await
                .map_err(ClientDiffSourceError::Desktop),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientDiffSourceError {
    Desktop(ClientApiError),
}

impl ClientDiffSourceError {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::Desktop(error) => error.message(),
        }
    }
}
