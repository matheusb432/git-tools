use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    File, Item, ItemFn, LitStr, Meta, ReturnType, Token, Type, parse::Parser,
    punctuated::Punctuated, spanned::Spanned,
};

use crate::{attributes, diagnostics, facade, metadata::MetadataArguments, naming};

#[derive(Default)]
struct VariantArguments {
    metadata: MetadataArguments,
    errors: Option<syn::Error>,
}

pub(crate) fn expand(arguments: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let function = syn::parse2::<ItemFn>(item)?;
    let mut errors = None;
    let arguments = diagnostics::capture(&mut errors, VariantArguments::parse(arguments));

    validate_signature(&function, &mut errors);
    if let Err(error) = attributes::validate(&function.attrs) {
        diagnostics::combine(&mut errors, error);
    }
    let derived_id = diagnostics::capture(&mut errors, naming::variant_id(&function.sig.ident));
    let facade = diagnostics::capture(&mut errors, facade::path(function.sig.ident.span()));
    diagnostics::finish(errors)?;

    let arguments =
        diagnostics::validated(arguments, "variant arguments", function.sig.ident.span())?;
    let facade = diagnostics::validated(facade, "storybook facade", function.sig.ident.span())?;
    let derived_id =
        diagnostics::validated(derived_id, "derived variant ID", function.sig.ident.span())?;
    let variant_const_ident = naming::variant_const_ident(&function.sig.ident);
    let component_ident = naming::variant_component_ident(&function.sig.ident);
    let variant_id = arguments
        .metadata
        .id
        .unwrap_or_else(|| LitStr::new(&derived_id, function.sig.ident.span()));
    let variant_name = arguments.metadata.name.unwrap_or_else(|| {
        LitStr::new(
            &naming::variant_name(&function.sig.ident),
            function.sig.ident.span(),
        )
    });
    let description = attributes::documentation(&function.attrs).map_or_else(
        || quote!(None),
        |description| {
            let description = LitStr::new(&description, function.sig.ident.span());
            quote!(Some(#description))
        },
    );
    let source = LitStr::new(&format_source(&function), function.sig.ident.span());
    let render_function = function;
    let function_ident = &render_function.sig.ident;
    let configuration_attributes = attributes::configuration(&render_function.attrs);
    let generated_attributes = attributes::generated(&render_function.attrs);

    Ok(quote! {
        #render_function

        #(#configuration_attributes)*
        #[allow(non_snake_case)]
        fn #component_ident() -> #facade::__private::dioxus::prelude::Element {
            #function_ident()
        }

        #(#generated_attributes)*
        const #variant_const_ident: &'static #facade::StoryVariant =
            &#facade::__private::story_variant(
                #variant_id,
                #variant_name,
                #description,
                || #facade::__private::dioxus::prelude::rsx! {
                    #component_ident {}
                },
                #source,
            );
    })
}

impl VariantArguments {
    fn parse(arguments: TokenStream) -> syn::Result<Self> {
        let parsed = Punctuated::<Meta, Token![,]>::parse_terminated.parse2(arguments)?;
        let mut arguments = Self::default();
        for argument in parsed {
            arguments.push(argument);
        }
        arguments.validate();
        diagnostics::finish(arguments.errors.take())?;
        Ok(arguments)
    }

    fn push(&mut self, argument: Meta) {
        let Meta::NameValue(argument) = argument else {
            diagnostics::combine(
                &mut self.errors,
                syn::Error::new_spanned(
                    argument,
                    "variant arguments must use `key = value` syntax",
                ),
            );
            return;
        };
        if !self.metadata.push(&argument, &mut self.errors) {
            diagnostics::combine(
                &mut self.errors,
                syn::Error::new_spanned(argument.path, "unknown variant argument"),
            );
        }
    }

    fn validate(&mut self) {
        self.metadata.validate("variant", &mut self.errors);
    }
}

fn validate_signature(function: &ItemFn, errors: &mut Option<syn::Error>) {
    if function.sig.constness.is_some() {
        diagnostics::combine(
            errors,
            syn::Error::new(function.sig.span(), "variant functions must not be const"),
        );
    }
    if function.sig.asyncness.is_some() {
        diagnostics::combine(
            errors,
            syn::Error::new(function.sig.span(), "variant functions must not be async"),
        );
    }
    if !matches!(function.sig.safety, syn::Safety::Default) {
        diagnostics::combine(
            errors,
            syn::Error::new(
                function.sig.span(),
                "variant functions must not use safety qualifiers",
            ),
        );
    }
    if function.sig.abi.is_some() {
        diagnostics::combine(
            errors,
            syn::Error::new(
                function.sig.span(),
                "variant functions must use the Rust ABI",
            ),
        );
    }
    if !function.sig.inputs.is_empty() {
        diagnostics::combine(
            errors,
            syn::Error::new_spanned(
                &function.sig.inputs,
                "variant functions must not accept arguments",
            ),
        );
    }
    if function.sig.variadic.is_some() {
        diagnostics::combine(
            errors,
            syn::Error::new_spanned(
                &function.sig.variadic,
                "variant functions must not be variadic",
            ),
        );
    }
    if !function.sig.generics.params.is_empty() || function.sig.generics.where_clause.is_some() {
        diagnostics::combine(
            errors,
            syn::Error::new_spanned(
                &function.sig.generics,
                "variant functions must not be generic",
            ),
        );
    }
    if !returns_element(&function.sig.output) {
        diagnostics::combine(
            errors,
            syn::Error::new_spanned(
                &function.sig.output,
                "variant functions must explicitly return `Element`",
            ),
        );
    }
}

fn returns_element(output: &ReturnType) -> bool {
    let ReturnType::Type(_, ty) = output else {
        return false;
    };
    let Type::Path(path) = &**ty else {
        return false;
    };
    path.qself.is_none()
        && path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Element" && segment.arguments.is_empty())
}

fn format_source(function: &ItemFn) -> String {
    prettyplease::unparse(&File {
        shebang: None,
        frontmatter: None,
        attrs: Vec::new(),
        items: vec![Item::Fn(function.clone())],
    })
}
