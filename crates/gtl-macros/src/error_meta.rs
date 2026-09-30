mod facade;
mod input;

use input::{ClassifiedVariant, ErrorMetaInput, VariantClassification};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::{DeriveInput, Ident, Path, Result, spanned::Spanned};

pub(super) fn expand(input: &DeriveInput) -> Result<TokenStream> {
    let input = ErrorMetaInput::parse(input)?;
    let ident = input.ident;
    let facade = &input.facade;
    let (impl_generics, type_generics, _) = input.generics.split_for_impl();
    let where_clause = classification_where_clause(&input);
    let arms = input
        .variants
        .iter()
        .map(|variant| variant_arm(variant.ident, &variant.classification, facade));

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #facade::failure::Classified for #ident #type_generics #where_clause {
            fn classify(&self) -> #facade::failure::Classification {
                match self {
                    #(#arms)*
                }
            }
        }
    })
}

fn variant_arm(
    ident: &Ident,
    classification: &VariantClassification<'_>,
    facade: &Path,
) -> TokenStream {
    let (pattern, value) = match classification {
        VariantClassification::Failure(field) => (
            field_pattern(ident, &field.member),
            quote! {
                #facade::failure::Classification::Public(
                    #facade::failure::PublicFailure::failure(inner),
                )
            },
        ),
        VariantClassification::FailureExpression(reason) => (
            quote!(Self::#ident { .. }),
            quote! {
                #facade::failure::Classification::Public(
                    ::core::convert::Into::<#facade::failure::Failure>::into(#reason),
                )
            },
        ),
        VariantClassification::Transparent(field) => (
            field_pattern(ident, &field.member),
            quote!(#facade::failure::Classified::classify(inner)),
        ),
        VariantClassification::Private(class) => (
            quote!(Self::#ident { .. }),
            quote! {
                #facade::failure::Classification::Private(
                    #facade::failure::ErrorClass::#class,
                )
            },
        ),
    };
    quote!(#pattern => #value,)
}

fn field_pattern(ident: &Ident, member: &syn::Member) -> TokenStream {
    quote!(Self::#ident { #member: inner, .. })
}

fn classification_where_clause(input: &ErrorMetaInput<'_>) -> TokenStream {
    let facade = &input.facade;
    let bounds: Vec<_> = input
        .variants
        .iter()
        .filter_map(|variant| classification_bound(variant, facade))
        .collect();
    if input.generics.where_clause.is_none() && bounds.is_empty() {
        return TokenStream::new();
    }
    let predicates = input
        .generics
        .where_clause
        .iter()
        .flat_map(|clause| &clause.predicates);
    quote!(where #(#predicates,)* #(#bounds,)*)
}

fn classification_bound(variant: &ClassifiedVariant<'_>, facade: &Path) -> Option<TokenStream> {
    let (field, trait_name) = match &variant.classification {
        VariantClassification::Failure(field) => (field, quote!(PublicFailure)),
        VariantClassification::Transparent(field) => (field, quote!(Classified)),
        VariantClassification::FailureExpression(_) | VariantClassification::Private(_) => {
            return None;
        }
    };
    let field_type = field.field_type;
    Some(quote_spanned!(field_type.span()=> #field_type: #facade::failure::#trait_name))
}
