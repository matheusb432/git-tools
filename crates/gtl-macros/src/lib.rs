//! Derive macros re-exported by `gtl_models`. Depend on `gtl-models`, not on this crate.

mod error_meta;

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
/// All other metadata belongs on variants; metadata on fields is rejected.
#[proc_macro_derive(ErrorMeta, attributes(meta))]
pub fn derive_error_meta(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    error_meta::expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
