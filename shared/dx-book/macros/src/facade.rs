use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

pub(crate) fn path(span: Span) -> syn::Result<TokenStream> {
    let found_crate = crate_name("dx-book").map_err(|error| {
        syn::Error::new(
            span,
            format!("could not resolve the dx-book facade crate: {error}"),
        )
    })?;

    Ok(match found_crate {
        FoundCrate::Itself => quote!(::dx_book),
        FoundCrate::Name(name) => {
            let ident = Ident::new(&name, span);
            quote!(::#ident)
        }
    })
}
