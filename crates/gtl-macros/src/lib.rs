//! Derive macros re-exported by `gtl_models`. Depend on `gtl-models`, not on this crate.

mod error_meta;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Implements `gtl_models::failure::Classified` from one `#[meta(...)]` attribute per variant.
///
/// - `#[meta(failure)]`: the variant's only field is a public reason implementing
///   `gtl_models::failure::PublicFailure`.
/// - `#[meta(failure = reason)]`: the variant always reports `reason`, any value convertible into
///   `gtl_models::failure::Failure`.
/// - `#[meta(transparent)]`: the variant's only field is itself `Classified`.
/// - `#[meta(private(Class))]`: only the `gtl_models::failure::ErrorClass` crosses the boundary.
#[proc_macro_derive(ErrorMeta, attributes(meta))]
pub fn derive_error_meta(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    error_meta::expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
