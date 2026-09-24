use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, Fields, Ident, Result, Token, Variant, parenthesized,
};

/// Mirrors `gtl_models::failure::ErrorClass`.
const ERROR_CLASSES: [&str; 15] = [
    "Cancelled",
    "InvalidArgument",
    "DeadlineExceeded",
    "NotFound",
    "AlreadyExists",
    "PermissionDenied",
    "ResourceExhausted",
    "FailedPrecondition",
    "Aborted",
    "OutOfRange",
    "Unimplemented",
    "Internal",
    "Unavailable",
    "DataLoss",
    "Unauthenticated",
];

const META_USAGE: &str = "expected `#[meta(failure)]`, `#[meta(failure = reason)]`, `#[meta(transparent)]`, or `#[meta(private(Class))]`";

enum VariantMeta {
    Failure,
    FixedFailure(Box<Expr>),
    Transparent,
    Private(Ident),
}

pub(crate) fn expand(input: &DeriveInput) -> Result<TokenStream> {
    let Data::Enum(data) = &input.data else {
        return Err(Error::new(
            input.ident.span(),
            "ErrorMeta can only be derived for enums",
        ));
    };
    let mut arms = Vec::with_capacity(data.variants.len());
    let mut errors: Option<Error> = None;
    for variant in &data.variants {
        match variant_arm(variant) {
            Ok(arm) => arms.push(arm),
            Err(error) => match &mut errors {
                Some(errors) => errors.combine(error),
                None => errors = Some(error),
            },
        }
    }
    if let Some(errors) = errors {
        return Err(errors);
    }

    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::gtl_models::failure::Classified for #ident #type_generics #where_clause {
            fn classify(&self) -> ::gtl_models::failure::Classification {
                match self {
                    #(#arms)*
                }
            }
        }
    })
}

fn variant_arm(variant: &Variant) -> Result<TokenStream> {
    let ident = &variant.ident;
    match variant_meta(variant)? {
        VariantMeta::Failure => {
            let pattern = single_field_pattern(variant, "failure")?;
            Ok(quote! {
                #pattern => ::gtl_models::failure::Classification::Public(
                    ::gtl_models::failure::PublicFailure::failure(inner),
                ),
            })
        }
        VariantMeta::FixedFailure(reason) => Ok(quote! {
            Self::#ident { .. } => ::gtl_models::failure::Classification::Public(
                ::gtl_models::failure::Failure::from(#reason),
            ),
        }),
        VariantMeta::Transparent => {
            let pattern = single_field_pattern(variant, "transparent")?;
            Ok(quote! {
                #pattern => ::gtl_models::failure::Classified::classify(inner),
            })
        }
        VariantMeta::Private(class) => Ok(quote! {
            Self::#ident { .. } => ::gtl_models::failure::Classification::Private(
                ::gtl_models::failure::ErrorClass::#class,
            ),
        }),
    }
}

fn variant_meta(variant: &Variant) -> Result<VariantMeta> {
    let mut attributes = variant
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("meta"));
    let Some(attribute) = attributes.next() else {
        return Err(Error::new(
            variant.ident.span(),
            format!(
                "variant `{}` needs a classification: {META_USAGE}",
                variant.ident
            ),
        ));
    };
    if let Some(duplicate) = attributes.next() {
        return Err(Error::new_spanned(
            duplicate,
            "each variant takes exactly one #[meta(...)] attribute",
        ));
    }
    parse_meta(attribute)
}

fn parse_meta(attribute: &Attribute) -> Result<VariantMeta> {
    let mut meta = None;
    attribute.parse_nested_meta(|nested| {
        if meta.is_some() {
            return Err(nested.error(META_USAGE));
        }
        if nested.path.is_ident("failure") {
            meta = Some(if nested.input.peek(Token![=]) {
                VariantMeta::FixedFailure(Box::new(nested.value()?.parse()?))
            } else {
                VariantMeta::Failure
            });
            return Ok(());
        }
        if nested.path.is_ident("transparent") {
            meta = Some(VariantMeta::Transparent);
            return Ok(());
        }
        if nested.path.is_ident("private") {
            let content;
            parenthesized!(content in nested.input);
            let class: Ident = content.parse()?;
            if !ERROR_CLASSES.contains(&class.to_string().as_str()) {
                return Err(Error::new(
                    class.span(),
                    format!(
                        "unknown error class `{class}`; expected one of {}",
                        ERROR_CLASSES.join(", ")
                    ),
                ));
            }
            meta = Some(VariantMeta::Private(class));
            return Ok(());
        }
        Err(nested.error(META_USAGE))
    })?;
    meta.ok_or_else(|| Error::new_spanned(attribute, META_USAGE))
}

fn single_field_pattern(variant: &Variant, meta: &str) -> Result<TokenStream> {
    let ident = &variant.ident;
    match &variant.fields {
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => Ok(quote!(Self::#ident(inner))),
        Fields::Named(fields) if fields.named.len() == 1 => {
            let name = &fields.named[0].ident;
            Ok(quote!(Self::#ident { #name: inner }))
        }
        _ => Err(Error::new(
            ident.span(),
            format!("`#[meta({meta})]` requires a variant with exactly one field"),
        )),
    }
}
