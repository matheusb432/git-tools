//! `ErrorMeta` behavior as seen by a downstream crate.

use gtl_models::failure::{
    Classification, Classified, ErrorClass, ErrorMeta, Failure, PushFailure, Resource,
};

mod facade {
    pub use gtl_models::failure;
}

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

#[derive(ErrorMeta)]
enum GenericError<'a, Reason, Inner, Context: ?Sized, const SIZE: usize>
where
    Context: 'a,
{
    #[meta(failure)]
    Reason(Reason),
    #[meta(transparent)]
    Inner { error: Inner },
    #[meta(private(Internal))]
    Context {
        _context: &'a Context,
        _bytes: [u8; SIZE],
    },
    #[meta(failure = PushFailure::ReviewExpired,)]
    Expired { _context: &'a Context },
}

#[test]
fn generics_preserve_bounds_and_ignore_private_fields() {
    struct Context;
    type Error<'a> = GenericError<'a, PushFailure, InnerError, Context, 2>;

    assert_eq!(
        Error::Reason(PushFailure::Detached).classify(),
        Classification::Public(Failure::Push(PushFailure::Detached))
    );
    assert_eq!(
        Error::Inner {
            error: InnerError::Capacity
        }
        .classify(),
        Classification::Private(ErrorClass::ResourceExhausted)
    );
    assert_eq!(
        Error::Context {
            _context: &Context,
            _bytes: [0; 2]
        }
        .classify(),
        Classification::Private(ErrorClass::Internal)
    );
    assert_eq!(
        Error::Expired { _context: &Context }.classify(),
        Classification::Public(Failure::Push(PushFailure::ReviewExpired))
    );
}

#[test]
fn delegation_bounds_apply_to_associated_field_types() {
    trait ReasonProvider {
        type Reason;
    }

    struct Provider;
    impl ReasonProvider for Provider {
        type Reason = PushFailure;
    }

    #[derive(ErrorMeta)]
    enum AssociatedError<Source = Provider>
    where
        Source: ReasonProvider,
    {
        #[meta(failure)]
        Reason(Source::Reason),
    }

    assert_eq!(
        AssociatedError::<Provider>::Reason(PushFailure::Detached).classify(),
        Classification::Public(Failure::Push(PushFailure::Detached))
    );
}

#[derive(ErrorMeta)]
enum SelectedError<Reason, Inner, Context> {
    #[meta(failure(field = reason))]
    ReasonNamed { reason: Reason, _context: Context },
    #[meta(failure(field = 1))]
    ReasonTuple(Context, Reason),
    #[meta(transparent(field = error))]
    InnerNamed { error: Inner, _context: Context },
    #[meta(transparent(field = 1))]
    InnerTuple(Context, Inner),
}

#[test]
fn selected_fields_delegate_without_classifying_context() {
    struct Context;
    type Error = SelectedError<PushFailure, InnerError, Context>;

    for error in [
        Error::ReasonNamed {
            reason: PushFailure::Detached,
            _context: Context,
        },
        Error::ReasonTuple(Context, PushFailure::Detached),
    ] {
        assert_eq!(
            error.classify(),
            Classification::Public(Failure::Push(PushFailure::Detached))
        );
    }
    for error in [
        Error::InnerNamed {
            error: InnerError::Capacity,
            _context: Context,
        },
        Error::InnerTuple(Context, InnerError::Capacity),
    ] {
        assert_eq!(
            error.classify(),
            Classification::Private(ErrorClass::ResourceExhausted)
        );
    }
}

#[test]
fn failure_expressions_accept_into_and_run_on_every_call() {
    use std::cell::Cell;

    struct Reason;
    #[expect(
        clippy::from_over_into,
        reason = "The derive must accept Into without a matching From implementation."
    )]
    impl Into<Failure> for Reason {
        fn into(self) -> Failure {
            Failure::Changed
        }
    }

    #[derive(ErrorMeta)]
    enum ExpressionError {
        #[meta(failure = self.reason())]
        Changed(Cell<usize>),
    }

    impl ExpressionError {
        fn reason(&self) -> Reason {
            let Self::Changed(calls) = self;
            calls.set(calls.get() + 1);
            Reason
        }
    }

    let error = ExpressionError::Changed(Cell::new(0));
    assert_eq!(error.classify(), Classification::Public(Failure::Changed));
    assert_eq!(error.classify(), Classification::Public(Failure::Changed));
    let ExpressionError::Changed(calls) = error;
    assert_eq!(calls.get(), 2);
}

#[test]
fn explicit_facade_paths_support_reexports_and_relative_modules() {
    mod models {
        pub use gtl_models::failure;
    }

    #[derive(ErrorMeta)]
    #[meta(crate = models)]
    enum LocalFacadeError {
        #[meta(private(Internal))]
        Internal,
    }

    #[derive(ErrorMeta)]
    #[meta(crate = self::facade)]
    enum ReexportedError {
        #[meta(private(Unavailable))]
        Unavailable,
    }

    assert_eq!(
        LocalFacadeError::Internal.classify(),
        Classification::Private(ErrorClass::Internal)
    );
    assert_eq!(
        ReexportedError::Unavailable.classify(),
        Classification::Private(ErrorClass::Unavailable)
    );
}

#[test]
fn failure_expressions_can_read_self() {
    #[derive(ErrorMeta)]
    enum RequestError {
        #[meta(failure = Failure::InvalidRequest { field: self.field().to_owned() })]
        Invalid { field: String },
    }

    impl RequestError {
        fn field(&self) -> &str {
            match self {
                Self::Invalid { field } => field,
            }
        }
    }

    assert_eq!(
        RequestError::Invalid {
            field: "identity".into()
        }
        .classify(),
        Classification::Public(Failure::InvalidRequest {
            field: "identity".into()
        })
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "compiler diagnostic fixtures run on the Ubuntu development host"
)]
fn invalid_declarations_fail_to_compile_with_spanned_diagnostics() {
    trybuild::TestCases::new().compile_fail("tests/error_meta_ui/*.rs");
}
