use syn::{Attribute, Expr, Lit, Meta, spanned::Spanned};

pub(crate) fn validate(attributes: &[Attribute]) -> syn::Result<()> {
    let mut errors = None;
    for attribute in attributes {
        if !is_supported(attribute) {
            crate::diagnostics::combine(
                &mut errors,
                syn::Error::new(
                    attribute.span(),
                    "storybook declarations support only doc, cfg, cfg_attr, and lint-level attributes",
                ),
            );
        }
    }
    crate::diagnostics::finish(errors)
}

pub(crate) fn generated(attributes: &[Attribute]) -> Vec<Attribute> {
    attributes
        .iter()
        .filter(|attribute| !attribute.path().is_ident("doc"))
        .cloned()
        .collect()
}

pub(crate) fn configuration(attributes: &[Attribute]) -> Vec<Attribute> {
    attributes
        .iter()
        .filter(|attribute| is_configuration(attribute))
        .cloned()
        .collect()
}

pub(crate) fn documentation(attributes: &[Attribute]) -> Option<String> {
    let documentation = attributes
        .iter()
        .filter_map(|attribute| {
            if !attribute.path().is_ident("doc") {
                return None;
            }
            let Meta::NameValue(meta) = &attribute.meta else {
                return None;
            };
            let Expr::Lit(expression) = &meta.value else {
                return None;
            };
            let Lit::Str(line) = &expression.lit else {
                return None;
            };
            Some(normalize_documentation_line(&line.value()))
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!documentation.trim().is_empty()).then_some(documentation)
}

fn is_supported(attribute: &Attribute) -> bool {
    attribute.path().is_ident("doc")
        || is_configuration(attribute)
        || ["allow", "warn", "deny", "forbid", "expect"]
            .iter()
            .any(|name| attribute.path().is_ident(name))
}

fn is_configuration(attribute: &Attribute) -> bool {
    attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr")
}

fn normalize_documentation_line(line: &str) -> String {
    line.strip_prefix(' ').unwrap_or(line).to_owned()
}
