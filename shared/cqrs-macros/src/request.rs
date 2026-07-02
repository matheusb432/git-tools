//! The pure, testable core of `#[derive(Request)]`: emit `impl Request` from `#[request(...)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, DeriveInput, Path, Token, Type};

use crate::mediator::pipeline_tuple;

/// Emits `impl ::cqrs::Request` for the annotated type from its `#[request(...)]` attribute.
pub(crate) fn expand_request(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let config = RequestConfig::from_attrs(input)?;
    let response = &config.response;
    let error = &config.error;
    let pipeline = pipeline_tuple(&config.behaviors);

    Ok(quote! {
        impl #impl_generics ::cqrs::Request for #name #ty_generics #where_clause {
            type Response = #response;
            type Error = #error;
            type Pipeline = #pipeline;
        }
    })
}

struct RequestConfig {
    response: Type,
    error: Type,
    behaviors: Vec<Path>,
}

impl RequestConfig {
    fn from_attrs(input: &DeriveInput) -> syn::Result<Self> {
        let mut response: Option<Type> = None;
        let mut error: Option<Type> = None;
        let mut behaviors: Vec<Path> = Vec::new();
        let mut seen: Option<&Attribute> = None;

        for attr in &input.attrs {
            if !attr.path().is_ident("request") {
                continue;
            }
            if seen.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "at most one #[request(...)] attribute is allowed",
                ));
            }
            seen = Some(attr);
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("response") {
                    response = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("error") {
                    error = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("with") {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let paths = content.parse_terminated(Path::parse_mod_style, Token![,])?;
                    if paths.is_empty() {
                        return Err(meta.error("with(...) requires at least one behavior path"));
                    }
                    behaviors = paths.into_iter().collect();
                    Ok(())
                } else {
                    Err(meta.error("expected `response`, `error`, or `with`"))
                }
            })?;
        }

        let missing = |what| {
            syn::Error::new_spanned(
                &input.ident,
                format!(
                    "derive(Request) requires #[request(response = ..., error = ...)]: missing `{what}`"
                ),
            )
        };
        Ok(Self {
            response: response.ok_or_else(|| missing("response"))?,
            error: error.ok_or_else(|| missing("error"))?,
            behaviors,
        })
    }
}

#[cfg(test)]
mod tests {
    use syn::{DeriveInput, parse_quote};

    use super::expand_request;

    #[test]
    fn generates_request_impl_with_empty_pipeline_by_default() {
        let input: DeriveInput = parse_quote! {
            #[request(response = Vec<TodoItem>, error = GetTodosError)]
            struct GetTodos;
        };
        let expanded = expand_request(&input)
            .expect("expansion succeeds")
            .to_string();
        assert!(expanded.contains("type Response = Vec < TodoItem >"));
        assert!(expanded.contains("type Error = GetTodosError"));
        assert!(expanded.contains("type Pipeline = ()"));
    }

    #[test]
    fn with_list_becomes_a_nested_pipeline_tuple() {
        let input: DeriveInput = parse_quote! {
            #[request(response = (), error = PingError, with(Logged, Timed))]
            struct Ping;
        };
        let expanded = expand_request(&input)
            .expect("expansion succeeds")
            .to_string();
        assert!(expanded.contains("type Pipeline = (Logged , (Timed , ()))"));
    }

    #[test]
    fn rejects_a_missing_response() {
        let input: DeriveInput = parse_quote! {
            #[request(error = PingError)]
            struct Ping;
        };
        assert!(
            expand_request(&input)
                .unwrap_err()
                .to_string()
                .contains("response")
        );
    }

    #[test]
    fn rejects_a_missing_request_attribute_entirely() {
        let input: DeriveInput = parse_quote! { struct Ping; };
        assert!(
            expand_request(&input)
                .unwrap_err()
                .to_string()
                .contains("#[request(")
        );
    }
}
