//! The single mapping from classified errors to gRPC statuses.

use std::fmt::Debug;

use gtl_models::failure::{Classification, Classified, ErrorClass, Failure};
use gtl_wire::proto::failure::encode_status;
use tonic::{Response, Status};

pub(crate) type ApiResult<T> = Result<Response<T>, Status>;

pub(crate) trait GrpcResultExt<T> {
    /// Maps the error through its declared classification.
    fn into_grpc(self) -> Result<T, Status>;
}

impl<T, E: Classified + Debug> GrpcResultExt<T> for Result<T, E> {
    fn into_grpc(self) -> Result<T, Status> {
        self.map_err(|error| status(&error))
    }
}

/// Encodes a public failure, or logs a private error once and encodes its generic reason.
pub(crate) fn status(error: &(impl Classified + Debug)) -> Status {
    match error.classify() {
        Classification::Public(failure) => {
            tracing::debug!(error = ?error, "gRPC request refused");
            encode_status(failure.class(), &failure)
        }
        Classification::Private(class) => private_status(class, error),
    }
}

/// Classifies an error that a response reports in band, logging private causes once.
pub(crate) fn failure(error: &(impl Classified + Debug)) -> Failure {
    match error.classify() {
        Classification::Public(failure) => {
            tracing::debug!(error = ?error, "operation refused");
            failure
        }
        Classification::Private(class) => {
            log_private(class, error);
            Failure::private(class)
        }
    }
}

/// Logs a private failure that has no error value and reports its generic reason.
pub(crate) fn private(class: ErrorClass, context: &'static str) -> Status {
    private_status(class, &context)
}

/// Logs a foreign error once and reports only the generic reason for `class`.
pub(crate) fn private_error(class: ErrorClass, error: &impl Debug) -> Status {
    private_status(class, error)
}

fn private_status(class: ErrorClass, error: &impl Debug) -> Status {
    log_private(class, error);
    encode_status(class, &Failure::private(class))
}

fn log_private(class: ErrorClass, error: &impl Debug) {
    if matches!(
        class,
        ErrorClass::Unavailable
            | ErrorClass::ResourceExhausted
            | ErrorClass::Aborted
            | ErrorClass::Cancelled
            | ErrorClass::DeadlineExceeded
    ) {
        tracing::warn!(error = ?error, ?class, "gRPC request failed");
    } else {
        tracing::error!(error = ?error, ?class, "gRPC request failed");
    }
}

/// Rejects a malformed request field.
pub(crate) fn invalid_request(field: &str) -> Status {
    status(&Failure::InvalidRequest {
        field: field.to_owned(),
    })
}

/// Decodes the typed failure a status carries, for assertions.
#[cfg(test)]
pub(crate) fn decoded_failure(status: &Status) -> Option<Failure> {
    match gtl_wire::proto::failure::decode_status(status) {
        gtl_wire::proto::failure::StatusFailure::Decoded(failure) => Some(failure),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::failure::{
        Classification, Classified, ErrorClass, Failure, PushFailure, SettingsFailure,
    };
    use gtl_wire::proto::failure::{StatusFailure, decode_status};
    use tonic::Code;

    use super::status;

    /// Stands in for an error whose text must stay in server logs.
    #[derive(Debug)]
    struct PrivateError;

    impl Classified for PrivateError {
        fn classify(&self) -> Classification {
            Classification::Private(ErrorClass::DataLoss)
        }
    }

    #[test]
    fn private_errors_keep_their_class_and_hide_their_text() {
        let status = status(&PrivateError);

        assert_eq!(status.code(), Code::DataLoss);
        assert!(!status.message().contains("PrivateError"));
        assert_eq!(
            decode_status(&status),
            StatusFailure::Decoded(Failure::Unexpected)
        );
    }

    #[test]
    fn public_failures_carry_their_reason_and_class() {
        let failure = Failure::Push(PushFailure::NothingToPush);

        let status = status(&failure);

        assert_eq!(status.code(), Code::FailedPrecondition);
        assert_eq!(decode_status(&status), StatusFailure::Decoded(failure));
    }

    #[test]
    fn invalid_settings_keep_the_settings_path_for_cli_output() {
        let failure = Failure::Settings(SettingsFailure::Invalid {
            path: "/home/user/.config/git-tools/config.toml".into(),
            diagnostic: gtl_models::failure::ExternalDiagnostic::new("expected `=`"),
        });

        let status = status(&failure);

        assert!(
            status
                .message()
                .contains("/home/user/.config/git-tools/config.toml")
        );
    }
}
