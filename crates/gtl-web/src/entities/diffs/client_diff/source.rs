use gtl_contracts::viewer::{LoadViewerDiffLines, ViewerDiffLines};

#[cfg(feature = "artifact")]
use self::artifact::{ArtifactDiffSourceError, load_artifact_diff_lines};
#[cfg(feature = "desktop")]
use super::super::api::DiffViewerApi;
#[cfg(feature = "desktop")]
use crate::shared::bridge::ClientApiError;

#[cfg(feature = "artifact")]
mod artifact;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientDiffSource {
    #[cfg(feature = "desktop")]
    Desktop,
    #[cfg(feature = "artifact")]
    Artifact,
}

impl ClientDiffSource {
    pub(super) async fn load_diff_lines(
        self,
        request: LoadViewerDiffLines,
    ) -> Result<ViewerDiffLines, ClientDiffSourceError> {
        match self {
            #[cfg(feature = "desktop")]
            Self::Desktop => DiffViewerApi::load_diff_lines(request)
                .await
                .map_err(ClientDiffSourceError::Desktop),
            #[cfg(feature = "artifact")]
            Self::Artifact => {
                load_artifact_diff_lines(&request).map_err(ClientDiffSourceError::Artifact)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientDiffSourceError {
    #[cfg(feature = "desktop")]
    Desktop(ClientApiError),
    #[cfg(feature = "artifact")]
    Artifact(ArtifactDiffSourceError),
}

impl ClientDiffSourceError {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            #[cfg(feature = "desktop")]
            Self::Desktop(error) => error.message(),
            #[cfg(feature = "artifact")]
            Self::Artifact(ArtifactDiffSourceError::Missing) => {
                "The embedded diff page is missing. Reload this artifact to try again."
            }
            #[cfg(feature = "artifact")]
            Self::Artifact(ArtifactDiffSourceError::Invalid) => {
                "The embedded diff page is invalid. Recreate this artifact to view it."
            }
        }
    }
}
