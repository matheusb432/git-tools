//! `ErrorMeta` behavior as seen by a downstream crate.

use gtl_models::failure::{
    Classification, Classified, ErrorClass, ErrorMeta, Failure, PushFailure, Resource,
};

#[derive(Debug, thiserror::Error, ErrorMeta)]
enum InnerError {
    #[error("inner capacity")]
    #[meta(private(ResourceExhausted))]
    Capacity,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
enum OperationError {
    #[error(transparent)]
    #[meta(failure)]
    Refused(#[from] PushFailure),
    #[error("state lock poisoned")]
    #[meta(private(Internal))]
    State,
    #[error("store failed: {reason}")]
    #[meta(private(Unavailable))]
    Store { reason: String },
    #[error(transparent)]
    #[meta(transparent)]
    Inner(#[from] InnerError),
    #[error(transparent)]
    #[meta(failure)]
    Named { failure: Failure },
    #[error("operation vanished")]
    #[meta(failure = Failure::Gone { resource: Resource::PushOperation })]
    Vanished,
    #[error("review expired at {0}")]
    #[meta(failure = PushFailure::ReviewExpired)]
    Expired(u64),
}

#[test]
fn variants_classify_as_declared() {
    assert_eq!(
        OperationError::Refused(PushFailure::Detached).classify(),
        Classification::Public(Failure::Push(PushFailure::Detached))
    );
    assert_eq!(
        OperationError::State.classify(),
        Classification::Private(ErrorClass::Internal)
    );
    assert_eq!(
        OperationError::Store {
            reason: "disk".into()
        }
        .classify(),
        Classification::Private(ErrorClass::Unavailable)
    );
    assert_eq!(
        OperationError::Inner(InnerError::Capacity).classify(),
        Classification::Private(ErrorClass::ResourceExhausted)
    );
    assert_eq!(
        OperationError::Named {
            failure: Failure::Changed
        }
        .classify(),
        Classification::Public(Failure::Changed)
    );
    assert_eq!(
        OperationError::Vanished.classify(),
        Classification::Public(Failure::Gone {
            resource: Resource::PushOperation
        })
    );
    assert_eq!(
        OperationError::Expired(7).classify(),
        Classification::Public(Failure::Push(PushFailure::ReviewExpired))
    );
}

#[test]
fn invalid_declarations_fail_to_compile_with_spanned_diagnostics() {
    trybuild::TestCases::new().compile_fail("tests/error_meta_ui/*.rs");
}
