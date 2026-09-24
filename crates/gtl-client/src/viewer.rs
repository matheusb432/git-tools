#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(any(test, all(target_arch = "wasm32", feature = "viewer-ipc")))]
mod stream_start;
#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
pub(crate) mod tauri;

use gtl_models::failure::{ErrorClass, ExternalDiagnostic, Failure};
#[cfg(not(target_arch = "wasm32"))]
pub use native::{ViewerClient, ViewerRowStream, ViewerVersionStream};
use serde::{Deserialize, Serialize};
#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
pub use tauri::pick_project_folder;
#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
pub use tauri::{ViewerClient, ViewerRowStream, ViewerVersionStream};

/// Why a viewer request failed, as seen by the `WebView`.
///
/// Server refusals and failures arrive as a typed [`Failure`]; the other variants describe the
/// local connection and protocol. Presentation renders the text; nothing here comes from the
/// server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ViewerClientError {
    /// The desktop viewer and gtl-server use different viewer protocol versions.
    #[error("This desktop viewer and gtl-server use different versions. Update and restart both.")]
    ProtocolMismatch,
    /// The connection to gtl-server is missing or broke.
    #[error("The desktop viewer is temporarily unavailable.")]
    Disconnected,
    /// A request or response could not be represented in the viewer protocol.
    #[error("The viewer could not read this response. Refresh and try again.")]
    InvalidMessage,
    /// A desktop row or version stream ended before this request.
    #[error("The viewer stream closed. Refresh to load it again.")]
    StreamClosed,
    /// A desktop window or dialog operation failed.
    #[error("The desktop window could not complete this action.")]
    Desktop { diagnostic: ExternalDiagnostic },
    /// The server refused or failed the request for a typed reason.
    #[error(transparent)]
    Failed(Failure),
}

impl ViewerClientError {
    /// A bounded local registry or queue is full.
    #[must_use]
    pub const fn busy() -> Self {
        Self::Failed(Failure::Busy)
    }

    /// A desktop window or dialog operation failed with `detail`.
    #[must_use]
    pub fn desktop(detail: &impl std::fmt::Display) -> Self {
        Self::Desktop {
            diagnostic: ExternalDiagnostic::new(&detail.to_string()),
        }
    }

    /// The failure category, for retry and recovery decisions.
    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::ProtocolMismatch => ErrorClass::FailedPrecondition,
            Self::Disconnected | Self::StreamClosed => ErrorClass::Unavailable,
            Self::InvalidMessage | Self::Desktop { .. } => ErrorClass::Internal,
            Self::Failed(failure) => failure.class(),
        }
    }

    /// Verbatim external output that explains the failure, shown apart from the message.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&ExternalDiagnostic> {
        match self {
            Self::Desktop { diagnostic } => Some(diagnostic),
            Self::Failed(failure) => failure.diagnostic(),
            _ => None,
        }
    }

    /// The server's typed reason, when the server produced the error.
    #[must_use]
    pub const fn failure(&self) -> Option<&Failure> {
        match self {
            Self::Failed(failure) => Some(failure),
            _ => None,
        }
    }
}

impl From<Failure> for ViewerClientError {
    fn from(failure: Failure) -> Self {
        Self::Failed(failure)
    }
}

pub(super) fn validate_viewer_protocol(protocol_version: u32) -> Result<(), ViewerClientError> {
    if protocol_version == gtl_wire::viewer::VIEWER_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ViewerClientError::ProtocolMismatch)
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::failure::{ErrorClass, Failure, PushFailure};

    use super::{ViewerClientError, validate_viewer_protocol};

    #[test]
    fn viewer_protocol_mismatch_has_a_clear_client_error() {
        let current = gtl_wire::viewer::VIEWER_PROTOCOL_VERSION;
        let different = current.checked_add(1).unwrap();

        assert_eq!(validate_viewer_protocol(current), Ok(()));
        assert_eq!(
            validate_viewer_protocol(different),
            Err(ViewerClientError::ProtocolMismatch)
        );
    }

    #[test]
    fn viewer_client_errors_round_trip_through_ipc_json() {
        for error in [
            ViewerClientError::ProtocolMismatch,
            ViewerClientError::Disconnected,
            ViewerClientError::InvalidMessage,
            ViewerClientError::Failed(Failure::Push(PushFailure::ReviewExpired)),
        ] {
            let encoded = serde_json::to_string(&error).unwrap();
            let decoded = serde_json::from_str::<ViewerClientError>(&encoded).unwrap();
            assert_eq!(decoded, error);
        }
    }

    #[test]
    fn server_failures_keep_their_class() {
        assert_eq!(
            ViewerClientError::Failed(Failure::Busy).class(),
            ErrorClass::ResourceExhausted
        );
        assert_eq!(
            ViewerClientError::Disconnected.class(),
            ErrorClass::Unavailable
        );
    }
}
