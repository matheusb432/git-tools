use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::Span;
use syn::{Error, Path, Result};

pub(super) fn resolve(path_override: Option<Path>, span: Span) -> Result<Path> {
    if let Some(path) = path_override {
        return Ok(path);
    }

    let name = match crate_name("gtl-models").map_err(|error| {
        Error::new(
            span,
            format!("cannot resolve gtl-models: {error}; specify #[meta(crate = path)]"),
        )
    })? {
        // The facade aliases itself so this path also works in its integration tests.
        FoundCrate::Itself => "gtl_models".to_owned(),
        FoundCrate::Name(name) => name,
    };
    syn::parse_str(&format!("::{name}")).or_else(|_| syn::parse_str(&format!("::r#{name}")))
}
