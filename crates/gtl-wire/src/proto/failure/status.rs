//! `google.rpc.Status` details carrying one `gtl.v1.Failure`.

use gtl_models::failure::{ErrorClass, Failure};
use prost::Message as _;
use tonic::Code;

use super::{decode_failure, encode_failure};
use crate::v1;

const FAILURE_TYPE_URL: &str = "type.googleapis.com/gtl.v1.Failure";

/// The typed failure a server attached to a status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusFailure {
    /// The status has no gtl failure detail, so a transport or framework produced it.
    Absent,
    /// A detail is present, but this build cannot decode its reason.
    Unrecognized,
    Decoded(Failure),
}

/// Builds the status for `failure`, reported with `class`.
///
/// The message is English diagnostics for logs and command-line output: the failure summary,
/// followed on the next line by any verbatim external diagnostic.
#[must_use]
pub fn encode_status(class: ErrorClass, failure: &Failure) -> tonic::Status {
    let code = class_code(class);
    let message = match failure.diagnostic() {
        Some(diagnostic) if !diagnostic.is_empty() => format!("{failure}\n{diagnostic}"),
        _ => failure.to_string(),
    };
    let details = tonic_types::pb::Status {
        code: code.into(),
        message: message.clone(),
        details: vec![prost_types::Any {
            type_url: FAILURE_TYPE_URL.to_owned(),
            value: encode_failure(failure).encode_to_vec(),
        }],
    };
    tonic::Status::with_details(code, message, details.encode_to_vec().into())
}

#[must_use]
pub fn decode_status(status: &tonic::Status) -> StatusFailure {
    let Ok(details) = tonic_types::pb::Status::decode(status.details()) else {
        return StatusFailure::Absent;
    };
    let Some(detail) = details
        .details
        .into_iter()
        .find(|detail| detail.type_url == FAILURE_TYPE_URL)
    else {
        return StatusFailure::Absent;
    };
    v1::Failure::decode(detail.value.as_slice())
        .ok()
        .and_then(decode_failure)
        .map_or(StatusFailure::Unrecognized, StatusFailure::Decoded)
}

#[must_use]
pub const fn class_code(class: ErrorClass) -> Code {
    match class {
        ErrorClass::Cancelled => Code::Cancelled,
        ErrorClass::InvalidArgument => Code::InvalidArgument,
        ErrorClass::DeadlineExceeded => Code::DeadlineExceeded,
        ErrorClass::NotFound => Code::NotFound,
        ErrorClass::AlreadyExists => Code::AlreadyExists,
        ErrorClass::PermissionDenied => Code::PermissionDenied,
        ErrorClass::ResourceExhausted => Code::ResourceExhausted,
        ErrorClass::FailedPrecondition => Code::FailedPrecondition,
        ErrorClass::Aborted => Code::Aborted,
        ErrorClass::OutOfRange => Code::OutOfRange,
        ErrorClass::Unimplemented => Code::Unimplemented,
        ErrorClass::Internal => Code::Internal,
        ErrorClass::Unavailable => Code::Unavailable,
        ErrorClass::DataLoss => Code::DataLoss,
        ErrorClass::Unauthenticated => Code::Unauthenticated,
    }
}

/// Maps a non-OK status code to its class; `Ok` and `Unknown` report `Internal`.
#[must_use]
pub const fn code_class(code: Code) -> ErrorClass {
    match code {
        Code::Cancelled => ErrorClass::Cancelled,
        Code::InvalidArgument => ErrorClass::InvalidArgument,
        Code::DeadlineExceeded => ErrorClass::DeadlineExceeded,
        Code::NotFound => ErrorClass::NotFound,
        Code::AlreadyExists => ErrorClass::AlreadyExists,
        Code::PermissionDenied => ErrorClass::PermissionDenied,
        Code::ResourceExhausted => ErrorClass::ResourceExhausted,
        Code::FailedPrecondition => ErrorClass::FailedPrecondition,
        Code::Aborted => ErrorClass::Aborted,
        Code::OutOfRange => ErrorClass::OutOfRange,
        Code::Unimplemented => ErrorClass::Unimplemented,
        Code::Unavailable => ErrorClass::Unavailable,
        Code::DataLoss => ErrorClass::DataLoss,
        Code::Unauthenticated => ErrorClass::Unauthenticated,
        Code::Ok | Code::Unknown | Code::Internal => ErrorClass::Internal,
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::failure::{ErrorClass, Failure, PushFailure};
    use prost::Message as _;
    use tonic::Code;

    use super::{StatusFailure, class_code, code_class, decode_status, encode_status};

    #[test]
    fn status_carries_code_summary_and_typed_failure() {
        let failure = Failure::Push(PushFailure::Detached);

        let status = encode_status(ErrorClass::FailedPrecondition, &failure);

        assert_eq!(status.code(), Code::FailedPrecondition);
        assert_eq!(status.message(), failure.to_string());
        assert_eq!(decode_status(&status), StatusFailure::Decoded(failure));
    }

    #[test]
    fn status_message_appends_external_diagnostics() {
        let failure = Failure::Push(PushFailure::GitFailed {
            diagnostic: gtl_models::failure::ExternalDiagnostic::new("fatal: bad object"),
        });

        let status = encode_status(ErrorClass::Internal, &failure);

        assert_eq!(status.message(), format!("{failure}\nfatal: bad object"));
    }

    #[test]
    fn details_follow_the_google_rpc_status_convention() {
        let status = encode_status(ErrorClass::Internal, &Failure::Unexpected);

        let details = tonic_types::pb::Status::decode(status.details()).unwrap();

        assert_eq!(details.code, i32::from(Code::Internal));
        assert_eq!(details.details.len(), 1);
        assert_eq!(
            details.details[0].type_url,
            "type.googleapis.com/gtl.v1.Failure"
        );
    }

    #[test]
    fn statuses_without_gtl_details_are_absent() {
        assert_eq!(
            decode_status(&tonic::Status::unavailable("connection refused")),
            StatusFailure::Absent
        );
    }

    #[test]
    fn unknown_reasons_in_gtl_details_are_unrecognized() {
        let details = tonic_types::pb::Status {
            code: Code::Internal.into(),
            message: String::new(),
            details: vec![prost_types::Any {
                type_url: "type.googleapis.com/gtl.v1.Failure".into(),
                value: crate::v1::Failure { reason: None }.encode_to_vec(),
            }],
        };
        let status =
            tonic::Status::with_details(Code::Internal, "", details.encode_to_vec().into());

        assert_eq!(decode_status(&status), StatusFailure::Unrecognized);
    }

    #[test]
    fn every_class_survives_its_status_code() {
        for class in [
            ErrorClass::Cancelled,
            ErrorClass::InvalidArgument,
            ErrorClass::DeadlineExceeded,
            ErrorClass::NotFound,
            ErrorClass::AlreadyExists,
            ErrorClass::PermissionDenied,
            ErrorClass::ResourceExhausted,
            ErrorClass::FailedPrecondition,
            ErrorClass::Aborted,
            ErrorClass::OutOfRange,
            ErrorClass::Unimplemented,
            ErrorClass::Internal,
            ErrorClass::Unavailable,
            ErrorClass::DataLoss,
            ErrorClass::Unauthenticated,
        ] {
            assert_eq!(code_class(class_code(class)), class);
        }
    }
}
