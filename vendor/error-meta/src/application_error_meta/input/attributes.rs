use syn::{Attribute, Error, Ident, Member, Path, Result, ext::IdentExt, meta::ParseNestedMeta};

use super::RawMetadata;

const ERROR_CLASSES: [&str; 5] = [
    "InvalidArgument",
    "NotFound",
    "FailedPrecondition",
    "Unavailable",
    "Internal",
];

impl RawMetadata {
    #[expect(
        clippy::excessive_nesting,
        reason = "keep the vendored attribute parser in sync with upstream"
    )]
    pub(super) fn parse(attribute: &Attribute) -> Result<Self> {
        let mut class = None;
        let mut reason = None;
        let mut transparent = None;
        attribute.parse_nested_meta(|nested| {
            if nested.path.is_ident("class") {
                if class.is_some() || transparent.is_some() {
                    return Err(nested.error("duplicate or conflicting class metadata"));
                }
                let value: Ident = nested.value()?.parse()?;
                if !ERROR_CLASSES.contains(&value.to_string().as_str()) {
                    return Err(Error::new(
                        value.span(),
                        format!(
                            "unknown application error class `{value}`; expected one of {}",
                            ERROR_CLASSES.join(", ")
                        ),
                    ));
                }
                class = Some(value);
            } else if nested.path.is_ident("reason") {
                if reason.is_some() || transparent.is_some() {
                    return Err(nested.error("duplicate or conflicting reason metadata"));
                }
                reason = Some(Box::new(nested.value()?.parse()?));
            } else if nested.path.is_ident("transparent") {
                if transparent.is_some() || class.is_some() || reason.is_some() {
                    return Err(nested.error("duplicate or conflicting transparent metadata"));
                }
                transparent = Some(field_selection(&nested)?);
            } else {
                return Err(nested.error("expected class, reason, or transparent metadata"));
            }
            Ok(())
        })?;
        if let Some(selection) = transparent {
            return Ok(Self::Transparent(selection));
        }
        match (class, reason) {
            (Some(class), Some(reason)) => Ok(Self::Fixed { class, reason }),
            _ => Err(Error::new_spanned(
                attribute,
                "fixed metadata requires both class and reason",
            )),
        }
    }
}

fn field_selection(nested: &ParseNestedMeta<'_>) -> Result<Option<Member>> {
    if !nested.input.peek(syn::token::Paren) {
        return Ok(None);
    }
    let mut selection = None;
    nested.parse_nested_meta(|field| {
        if !field.path.is_ident("field") || selection.is_some() {
            return Err(field.error("expected one field = name or field = index selection"));
        }
        selection = Some(field.value()?.parse()?);
        Ok(())
    })?;
    selection
        .map(Some)
        .ok_or_else(|| nested.error("expected one field = name or field = index selection"))
}

pub(super) fn is_facade(attribute: &Attribute) -> bool {
    attribute
        .parse_args_with(|input: syn::parse::ParseStream<'_>| {
            let key = input.call(Ident::parse_any)?;
            let _: proc_macro2::TokenStream = input.parse()?;
            Ok(key == "crate")
        })
        .unwrap_or(false)
}

pub(super) fn facade(attribute: &Attribute, duplicate: bool) -> Result<Path> {
    if duplicate {
        return Err(Error::new_spanned(
            attribute,
            "each item takes at most one #[meta(crate = path)] attribute",
        ));
    }
    let mut path = None;
    attribute.parse_nested_meta(|nested| {
        if !nested.path.is_ident("crate") || path.is_some() {
            return Err(nested.error("facade metadata only supports #[meta(crate = path)]"));
        }
        path = Some(nested.value()?.parse()?);
        Ok(())
    })?;
    path.ok_or_else(|| Error::new_spanned(attribute, "expected #[meta(crate = path)]"))
}
