//! Decoding of failed gtl-server requests.

use gtl_models::failure::Failure;
use gtl_wire::proto;
use tonic::{Code, Status};

/// What a failed gtl-server request reports to its caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestFailure {
    /// The server answered with a typed failure.
    Failed(Failure),
    /// The connection to the server failed before it answered.
    Disconnected,
}

impl RequestFailure {
    /// Decodes the typed failure a gtl server attached to `status`.
    ///
    /// A status without gtl details comes from the transport, so connection-shaped codes mean the
    /// server is unreachable.
    pub(crate) fn from_status(status: &Status) -> Self {
        let class = proto::failure::code_class(status.code());
        match proto::failure::decode_status(status) {
            proto::failure::StatusFailure::Decoded(failure) => Self::Failed(failure),
            proto::failure::StatusFailure::Unrecognized => {
                Self::Failed(Failure::Unrecognized { class })
            }
            proto::failure::StatusFailure::Absent => match status.code() {
                Code::Unavailable
                | Code::DeadlineExceeded
                | Code::Cancelled
                | Code::Unknown
                | Code::Unauthenticated => Self::Disconnected,
                _ => Self::Failed(Failure::Unrecognized { class }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::failure::{ErrorClass, ExternalDiagnostic, SettingsFailure};

    use super::*;

    #[test]
    fn typed_server_failures_survive_decoding() {
        let failure = Failure::Settings(SettingsFailure::Invalid {
            path: "/home/user/.config/git-tools/config.toml".into(),
            diagnostic: ExternalDiagnostic::new("expected `=`"),
        });
        let status = proto::failure::encode_status(failure.class(), &failure);

        assert_eq!(
            RequestFailure::from_status(&status),
            RequestFailure::Failed(failure)
        );
    }

    #[test]
    fn transport_statuses_without_details_mean_disconnected() {
        assert_eq!(
            RequestFailure::from_status(&Status::unavailable("connection refused")),
            RequestFailure::Disconnected
        );
        assert_eq!(
            RequestFailure::from_status(&Status::aborted("framework conflict")),
            RequestFailure::Failed(Failure::Unrecognized {
                class: ErrorClass::Aborted
            })
        );
    }
}
