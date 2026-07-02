//! The pure, testable core of `#[derive(Mediator)]`: given a struct's `DeriveInput`, emit one
//! `impl RequestHandler<X>` per `#[handles(X)]`-annotated field, dispatching through the
//! struct's optional `#[with(...)]` global behavior pipeline.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Field, Fields, Path, Token, punctuated::Punctuated,
    spanned::Spanned,
};

/// Emits one `impl ::cqrs::RequestHandler<X> for TheStruct` per `#[handles(X)]` field.
///
/// A field with no `#[handles(...)]` attribute is skipped. Every generated body constructs the
/// struct-level `#[with(...)]` global pipeline (or `()` if absent) and routes through
/// `::cqrs::dispatch`, which layers in the request's own `Request::Pipeline` before the handler.
pub(crate) fn expand_mediator(input: &DeriveInput) -> syn::Result<TokenStream> {
    let struct_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let global = pipeline_tuple(&parse_with_paths(&input.attrs)?);

    let mut impls = Vec::new();
    for field in named_fields(input)? {
        let Some(request_ty) = handled_request(field)? else {
            continue;
        };
        let field_ident = field
            .ident
            .as_ref()
            .expect("named_fields only yields fields with an ident");

        // ! Assumes the facade crate is imported as `cqrs` — a renamed dependency (`package =
        // "cqrs"`) would break this path.
        impls.push(quote! {
            impl #impl_generics ::cqrs::RequestHandler<#request_ty> for #struct_name #ty_generics #where_clause {
                async fn handle(
                    &self,
                    req: #request_ty,
                ) -> ::core::result::Result<
                    <#request_ty as ::cqrs::Request>::Response,
                    <#request_ty as ::cqrs::Request>::Error,
                > {
                    let global: #global = ::core::default::Default::default();
                    ::cqrs::dispatch(&global, &self.#field_ident, req).await
                }
            }
        });
    }

    Ok(quote! { #(#impls)* })
}

/// Folds `[A, B]` into the nested pipeline tuple type `(A, (B, ()))`; `[]` folds to `()`.
pub(crate) fn pipeline_tuple(paths: &[Path]) -> TokenStream {
    paths
        .iter()
        .rev()
        .fold(quote!(()), |acc, path| quote!((#path, #acc)))
}

/// Parses at most one `#[with(B1, B2, …)]` attribute into its behavior paths.
pub(crate) fn parse_with_paths(attrs: &[Attribute]) -> syn::Result<Vec<Path>> {
    let mut found: Option<Vec<Path>> = None;
    for attr in attrs {
        if !attr.path().is_ident("with") {
            continue;
        }
        if found.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "at most one #[with(...)] attribute is allowed",
            ));
        }
        let paths = attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)?;
        if paths.is_empty() {
            return Err(syn::Error::new_spanned(
                attr,
                "#[with(...)] requires at least one behavior path",
            ));
        }
        found = Some(paths.into_iter().collect());
    }
    Ok(found.unwrap_or_default())
}

/// Returns the struct's named fields, or an error if it isn't a named-field struct.
fn named_fields(
    input: &DeriveInput,
) -> syn::Result<&syn::punctuated::Punctuated<Field, syn::Token![,]>> {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => Ok(&named.named),
            other => Err(syn::Error::new(
                other.span(),
                "cqrs::Mediator only supports structs with named fields",
            )),
        },
        _ => Err(syn::Error::new_spanned(
            input.ident.clone(),
            "cqrs::Mediator can only be derived for structs",
        )),
    }
}

/// Returns the single request type named in a field's `#[handles(...)]` attribute, or `None`
/// if the field has no such attribute. Errors if the attribute doesn't contain exactly one path
/// (syn's `parse_args` naturally rejects zero paths or trailing tokens after one path).
fn handled_request(field: &Field) -> syn::Result<Option<Path>> {
    let mut found: Option<Path> = None;
    for attr in &field.attrs {
        if !attr.path().is_ident("handles") {
            continue;
        }
        let request_ty = attr.parse_args::<Path>()?;
        if found.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "a field may have at most one #[handles(...)] attribute",
            ));
        }
        found = Some(request_ty);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use syn::{DeriveInput, ItemImpl, parse::Parse, parse_quote, parse2};

    use super::expand_mediator;

    /// Parses the generator's output back into a list of `impl` items, so tests assert on
    /// structure (trait path, generic argument, method body) instead of brittle token strings.
    struct Impls(Vec<ItemImpl>);

    impl Parse for Impls {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let mut impls = Vec::new();
            while !input.is_empty() {
                impls.push(input.parse()?);
            }
            Ok(Impls(impls))
        }
    }

    fn trait_last_segment(item: &ItemImpl) -> String {
        let (_, path, _) = item.trait_.as_ref().expect("generated impl has a trait");
        path.segments.last().unwrap().ident.to_string()
    }

    #[test]
    fn generates_one_impl_per_handled_field() {
        let input: DeriveInput = parse_quote! {
            struct Mediator<R> where R: Clone {
                #[handles(GetTodoById)]
                get_by_id: GetTodoByIdHandler<R>,
                #[handles(UpdateTodo)]
                update: UpdateTodoHandler<R>,
            }
        };

        let expanded = expand_mediator(&input).expect("expansion succeeds");
        let Impls(impls) = parse2(expanded).expect("output parses as a list of impl items");

        assert_eq!(impls.len(), 2);
        assert_eq!(trait_last_segment(&impls[0]), "RequestHandler");
        assert_eq!(trait_last_segment(&impls[1]), "RequestHandler");
    }

    #[test]
    fn skips_fields_without_a_handles_attribute() {
        let input: DeriveInput = parse_quote! {
            struct Mediator {
                #[handles(Ping)]
                ping: PingHandler,
                clock: SystemClock,
            }
        };

        let expanded = expand_mediator(&input).expect("expansion succeeds");
        let Impls(impls) = parse2(expanded).expect("output parses as a list of impl items");

        assert_eq!(impls.len(), 1);
    }

    #[test]
    fn rejects_tuple_structs() {
        let input: DeriveInput = parse_quote! {
            struct Mediator(u8);
        };

        let err = expand_mediator(&input).expect_err("tuple structs are rejected");
        assert!(err.to_string().contains("named fields"));
    }

    #[test]
    fn rejects_a_handles_attribute_with_more_than_one_path() {
        let input: DeriveInput = parse_quote! {
            struct Mediator {
                #[handles(A, B)]
                handler: SomeHandler,
            }
        };

        assert!(expand_mediator(&input).is_err());
    }

    #[test]
    fn rejects_a_field_with_two_separate_handles_attributes() {
        let input: DeriveInput = parse_quote! {
            struct Mediator {
                #[handles(A)]
                #[handles(B)]
                handler: SomeHandler,
            }
        };

        let err =
            expand_mediator(&input).expect_err("two separate handles attributes are rejected");
        assert!(err.to_string().contains("at most one"));
    }

    #[test]
    fn with_attribute_produces_a_nested_global_tuple_in_every_body() {
        let input: DeriveInput = parse_quote! {
            #[with(Logged, Timed)]
            struct Mediator {
                #[handles(Ping)]
                ping: PingHandler,
            }
        };

        let expanded = expand_mediator(&input)
            .expect("expansion succeeds")
            .to_string();
        assert!(expanded.contains("(Logged , (Timed , ()))"));
        assert!(expanded.contains(":: cqrs :: dispatch"));
    }

    #[test]
    fn rejects_two_with_attributes() {
        let input: DeriveInput = parse_quote! {
            #[with(A)]
            #[with(B)]
            struct Mediator {
                #[handles(Ping)]
                ping: PingHandler,
            }
        };

        assert!(
            expand_mediator(&input)
                .unwrap_err()
                .to_string()
                .contains("at most one")
        );
    }

    #[test]
    fn rejects_an_empty_with_attribute() {
        let input: DeriveInput = parse_quote! {
            #[with()]
            struct Mediator {
                #[handles(Ping)]
                ping: PingHandler,
            }
        };

        assert!(
            expand_mediator(&input)
                .unwrap_err()
                .to_string()
                .contains("at least one")
        );
    }
}
