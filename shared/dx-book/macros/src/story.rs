use std::collections::BTreeMap;

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{
    Expr, ExprArray, ItemConst, LitStr, Meta, MetaNameValue, Path, Token, Type, ext::IdentExt,
    parse::Parser, punctuated::Punctuated, spanned::Spanned,
};

use crate::{attributes, diagnostics, facade, metadata::MetadataArguments, naming};

struct StoryArguments {
    id: LitStr,
    name: LitStr,
    extra_docs: Option<Path>,
    preview: Option<Ident>,
}

#[derive(Default)]
struct StoryArgumentsBuilder {
    metadata: MetadataArguments,
    extra_docs: Option<Path>,
    preview: Option<Ident>,
    errors: Option<syn::Error>,
}

pub(crate) fn expand(arguments: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let item = syn::parse2::<ItemConst>(item)?;
    let mut errors = None;
    let arguments = diagnostics::capture(&mut errors, StoryArguments::parse(arguments));
    let variant_idents = diagnostics::capture(&mut errors, parse_variant_idents(&item));
    if let Err(error) = validate_unit_type(&item.ty) {
        diagnostics::combine(&mut errors, error);
    }
    if let Err(error) = attributes::validate(&item.attrs) {
        diagnostics::combine(&mut errors, error);
    }
    let facade = diagnostics::capture(&mut errors, facade::path(item.ident.span()));
    diagnostics::finish(errors)?;

    let arguments = diagnostics::validated(arguments, "story arguments", item.ident.span())?;
    let variant_idents =
        diagnostics::validated(variant_idents, "story variants", item.ident.span())?;
    let facade = diagnostics::validated(facade, "storybook facade", item.ident.span())?;
    let story_const_ident = naming::story_const_ident(&item.ident);
    let documentation_const_ident = Ident::new(
        &format!("{story_const_ident}_DOCUMENTATION"),
        item.ident.span(),
    );
    let variant_const_idents = variant_idents
        .iter()
        .map(naming::variant_const_ident)
        .collect::<Vec<_>>();
    let preview = arguments.preview.as_ref().map(naming::variant_const_ident);
    let preview = preview.map_or_else(|| quote!(None), |preview| quote!(Some(#preview)));
    let description = description_expression(
        &facade,
        attributes::documentation(&item.attrs),
        arguments.extra_docs.as_ref(),
        item.ident.span(),
    );
    let story_id = arguments.id;
    let story_name = arguments.name;
    let generated_attributes = attributes::generated(&item.attrs);
    let configuration_attributes = attributes::configuration(&item.attrs);

    Ok(quote! {
        #(#generated_attributes)*
        const #documentation_const_ident: Option<&'static str> = #description;

        #(#generated_attributes)*
        const #story_const_ident: &'static #facade::Story =
            &#facade::__private::story(
                #story_id,
                #story_name,
                #documentation_const_ident,
                #preview,
                &[#(#variant_const_idents),*],
            );

        #(#configuration_attributes)*
        #facade::__private::submit! {
            #story_const_ident
        }
    })
}

impl StoryArguments {
    fn parse(arguments: TokenStream) -> syn::Result<Self> {
        let parsed = Punctuated::<Meta, Token![,]>::parse_terminated.parse2(arguments)?;
        let mut builder = StoryArgumentsBuilder::default();
        for argument in parsed {
            builder.push(argument);
        }
        builder.finish()
    }
}

impl StoryArgumentsBuilder {
    fn push(&mut self, argument: Meta) {
        let Meta::NameValue(argument) = argument else {
            diagnostics::combine(
                &mut self.errors,
                syn::Error::new_spanned(argument, "story arguments must use `key = value` syntax"),
            );
            return;
        };

        if self.metadata.push(&argument, &mut self.errors) {
            return;
        }
        if argument.path.is_ident("extra_docs") {
            self.parse_extra_docs(argument);
        } else if argument.path.is_ident("preview") {
            self.parse_preview(argument);
        } else {
            diagnostics::combine(
                &mut self.errors,
                syn::Error::new_spanned(argument.path, "unknown story argument"),
            );
        }
    }

    fn parse_extra_docs(&mut self, argument: MetaNameValue) {
        if self.extra_docs.is_some() {
            diagnostics::combine(
                &mut self.errors,
                syn::Error::new_spanned(argument, "duplicate `extra_docs` argument"),
            );
            return;
        }

        if let Expr::Path(path) = argument.value {
            self.extra_docs = Some(path.path);
        } else {
            diagnostics::combine(
                &mut self.errors,
                syn::Error::new_spanned(
                    argument.value,
                    "`extra_docs` must be a path to a `&'static str` constant",
                ),
            );
        }
    }

    fn parse_preview(&mut self, argument: MetaNameValue) {
        if self.preview.is_some() {
            diagnostics::combine(
                &mut self.errors,
                syn::Error::new_spanned(argument, "duplicate `preview` argument"),
            );
            return;
        }
        let Expr::Path(path) = argument.value else {
            diagnostics::combine(
                &mut self.errors,
                syn::Error::new_spanned(
                    argument.value,
                    "`preview` must be an unqualified variant function name",
                ),
            );
            return;
        };
        let Some(ident) = path.path.get_ident() else {
            diagnostics::combine(
                &mut self.errors,
                syn::Error::new_spanned(
                    path,
                    "`preview` must be an unqualified variant function name",
                ),
            );
            return;
        };
        self.preview = Some(ident.clone());
    }

    fn finish(mut self) -> syn::Result<StoryArguments> {
        self.metadata.id = required_argument(self.metadata.id, "id", &mut self.errors);
        self.metadata.name = required_argument(self.metadata.name, "name", &mut self.errors);
        self.metadata.validate("story", &mut self.errors);

        diagnostics::finish(self.errors)?;
        Ok(StoryArguments {
            id: diagnostics::validated(self.metadata.id, "story ID", Span::call_site())?,
            name: diagnostics::validated(self.metadata.name, "story name", Span::call_site())?,
            extra_docs: self.extra_docs,
            preview: self.preview,
        })
    }
}

fn required_argument(
    value: Option<LitStr>,
    name: &str,
    errors: &mut Option<syn::Error>,
) -> Option<LitStr> {
    if value.is_none() {
        diagnostics::combine(
            errors,
            syn::Error::new(
                Span::call_site(),
                format!("story requires the `{name}` argument"),
            ),
        );
    }
    value
}

fn parse_variant_idents(item: &ItemConst) -> syn::Result<Vec<Ident>> {
    let array = match &*item.expr {
        Expr::Array(array) => array,
        Expr::Reference(reference) => match &*reference.expr {
            Expr::Array(array) => array,
            expression => {
                return Err(syn::Error::new_spanned(
                    expression,
                    "story const must reference an array of variant function names",
                ));
            }
        },
        expression => {
            return Err(syn::Error::new_spanned(
                expression,
                "story const must contain an array of variant function names",
            ));
        }
    };
    extract_variant_idents(array)
}

fn extract_variant_idents(array: &ExprArray) -> syn::Result<Vec<Ident>> {
    let mut variants = Vec::with_capacity(array.elems.len());
    let mut first_occurrences = BTreeMap::new();
    let mut errors = None;

    if array.elems.is_empty() {
        diagnostics::combine(
            &mut errors,
            syn::Error::new(array.span(), "story must contain at least one variant"),
        );
    }

    for expression in &array.elems {
        let Expr::Path(path) = expression else {
            diagnostics::combine(
                &mut errors,
                syn::Error::new_spanned(expression, "expected a variant function name"),
            );
            continue;
        };
        let Some(ident) = path.path.get_ident() else {
            diagnostics::combine(
                &mut errors,
                syn::Error::new_spanned(
                    path,
                    "variant function names must be unqualified identifiers",
                ),
            );
            continue;
        };
        let canonical = ident.unraw().to_string();
        if first_occurrences
            .insert(canonical.clone(), ident.span())
            .is_some()
        {
            diagnostics::combine(
                &mut errors,
                syn::Error::new(
                    ident.span(),
                    format!("duplicate variant `{canonical}` in story declaration"),
                ),
            );
        }
        variants.push(ident.clone());
    }

    diagnostics::finish(errors)?;
    Ok(variants)
}

fn validate_unit_type(ty: &Type) -> syn::Result<()> {
    if let Type::Tuple(tuple) = ty
        && tuple.elems.is_empty()
    {
        return Ok(());
    }

    Err(syn::Error::new_spanned(ty, "story const type must be `()`"))
}

fn description_expression(
    facade: &TokenStream,
    documentation: Option<String>,
    extra_docs: Option<&Path>,
    span: Span,
) -> TokenStream {
    match (documentation, extra_docs) {
        (Some(documentation), Some(extra_docs)) => {
            let documentation = LitStr::new(&documentation, span);
            quote!(Some(#facade::__private::concatcp!(#documentation, "\n", #extra_docs)))
        }
        (Some(documentation), None) => {
            let documentation = LitStr::new(&documentation, span);
            quote!(Some(#documentation))
        }
        (None, Some(extra_docs)) => quote!(Some(#extra_docs)),
        (None, None) => quote!(None),
    }
}
