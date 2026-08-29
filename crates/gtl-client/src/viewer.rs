#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
mod tauri;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{ViewerClient, ViewerRowStream, ViewerVersionStream};
use serde::{Deserialize, Serialize};
#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
pub use tauri::{ViewerClient, ViewerRowStream, ViewerVersionStream};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ViewerClientError {
    #[error("This desktop viewer and gtl-server use different viewer protocol versions.")]
    ProtocolMismatch,
    #[error("The viewer rejected this request. Refresh the page and try again.")]
    InvalidRequest,
    #[error("This viewer item is no longer available.")]
    NotFound,
    #[error("The viewer changed while this action was running. Try again.")]
    Conflict,
    #[error("The viewer has too many active streams. Close another viewer and try again.")]
    ResourceExhausted,
    #[error("The desktop viewer is temporarily unavailable.")]
    Unavailable,
    #[error("The viewer could not complete this action.")]
    Internal,
}

impl ViewerClientError {
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::ProtocolMismatch => {
                "This desktop viewer and gtl-server use different versions. Update and restart both."
            }
            Self::InvalidRequest => {
                "The viewer rejected this request. Refresh the page and try again."
            }
            Self::NotFound => "This viewer item is no longer available.",
            Self::Conflict => "The viewer changed while this action was running. Try again.",
            Self::ResourceExhausted => {
                "The viewer has too many active streams. Close another viewer and try again."
            }
            Self::Unavailable => "The desktop viewer is temporarily unavailable.",
            Self::Internal => "The viewer could not complete this action.",
        }
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
    fn viewer_client_errors_round_trip_as_stable_wire_values() {
        for error in [
            ViewerClientError::ProtocolMismatch,
            ViewerClientError::InvalidRequest,
            ViewerClientError::NotFound,
            ViewerClientError::Conflict,
            ViewerClientError::ResourceExhausted,
            ViewerClientError::Unavailable,
            ViewerClientError::Internal,
        ] {
            let encoded = serde_json::to_string(&error).unwrap();
            let decoded = serde_json::from_str::<ViewerClientError>(&encoded).unwrap();
            assert_eq!(decoded, error);
        }
    }
}
