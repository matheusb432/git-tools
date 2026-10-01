//! Derive error classifications against a caller-owned failure facade.

mod application_error_meta;
mod error_meta;
mod facade;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Implements `gtl_models::failure::Classified` from one `#[meta(...)]` attribute per variant.
///
/// - `#[meta(failure)]`: the variant's only field is a public reason implementing
///   `gtl_models::failure::PublicFailure`.
/// - `#[meta(failure = expr)]`: evaluates `expr` on every `classify()` call and converts its result
///   with `Into<gtl_models::failure::Failure>`. The expression can use `self` and surrounding
///   names; the variant's fields are not bound to local names.
/// - `#[meta(transparent)]`: the variant's only field is itself `Classified`.
/// - `#[meta(private(Class))]`: only the `gtl_models::failure::ErrorClass` crosses the boundary.
///
/// Delegation can select one field from a variant with other context fields:
/// `#[meta(failure(field = reason))]` or `#[meta(transparent(field = source))]` for named fields,
/// and `#[meta(failure(field = 0))]` or `#[meta(transparent(field = 1))]` for tuple fields.
/// The generated implementation adds `PublicFailure` or `Classified` bounds only for delegated
/// field types, preserving the enum's declared bounds and leaving context fields unconstrained.
///
/// The derive resolves the `gtl-models` dependency name, including Cargo aliases. An enum-level
/// `#[meta(crate = path)]` overrides the facade path; that path must expose the `failure` module
/// and its `Classified`, `PublicFailure`, `Classification`, `Failure`, and `ErrorClass` contracts.
/// Automatic discovery is enabled by default through the `crate-discovery` feature. Without
/// that feature, every enum requires an explicit facade path.
/// All other metadata belongs on variants; metadata on fields is rejected.
#[proc_macro_derive(ErrorMeta, attributes(meta))]
pub fn derive_error_meta(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    error_meta::expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Implements a caller-owned `error::ApplicationError` for enums and structs.
///
/// Each variant (or struct) declares `#[meta(class = Class, reason = expression)]`
/// or delegates with `#[meta(transparent)]`. Delegation can select a named or tuple
/// field with `#[meta(transparent(field = source))]` or `field = 0`.
/// Only delegated field types receive `ApplicationError` bounds. The complete
/// type must implement `std::error::Error`; reason expressions must be static strings.
///
/// Resolves the `tufo-application` Cargo dependency, including aliases, by default.
/// `#[meta(crate = path)]` overrides the facade, which must expose an `error` module
/// containing `ApplicationError`, `ErrorMetadata`, and the five application classes:
/// `InvalidArgument`, `NotFound`, `FailedPrecondition`, `Unavailable`, and `Internal`.
#[proc_macro_derive(ApplicationErrorMeta, attributes(meta))]
pub fn derive_application_error_meta(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    application_error_meta::expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
