use syn::{
    Attribute, Error, Ident, Member, Path, Result, Token, meta::ParseNestedMeta, parenthesized,
};

use super::VariantMeta;

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

pub(super) const META_USAGE: &str = "expected `#[meta(failure)]`, `#[meta(failure = reason)]`, `#[meta(transparent)]`, or `#[meta(private(Class))]`";

const FIELD_USAGE: &str = "expected one `field = name` or `field = index` selection";

impl VariantMeta {
    pub(super) fn parse(attribute: &Attribute) -> Result<Self> {
        let mut meta = None;
        attribute.parse_nested_meta(|nested| {
            if meta.is_some() {
                return Err(nested.error(META_USAGE));
            }
            meta = Some(Self::parse_nested(&nested)?);
            Ok(())
        })?;
        meta.ok_or_else(|| Error::new_spanned(attribute, META_USAGE))
    }

    fn parse_nested(nested: &ParseNestedMeta<'_>) -> Result<Self> {
        if nested.path.is_ident("failure") {
            return Ok(if nested.input.peek(Token![=]) {
                Self::FailureExpression(Box::new(nested.value()?.parse()?))
            } else {
                Self::Failure(field_selection(nested)?)
            });
        }
        if nested.path.is_ident("transparent") {
            return Ok(Self::Transparent(field_selection(nested)?));
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
            return Ok(Self::Private(class));
        }
        Err(nested.error(META_USAGE))
    }
}

fn field_selection(nested: &ParseNestedMeta<'_>) -> Result<Option<Member>> {
    if !nested.input.peek(syn::token::Paren) {
        return Ok(None);
    }
    let mut selection = None;
    nested.parse_nested_meta(|field| {
        if !field.path.is_ident("field") || selection.is_some() {
            return Err(field.error(FIELD_USAGE));
        }
        selection = Some(field.value()?.parse()?);
        Ok(())
    })?;
    selection.map(Some).ok_or_else(|| nested.error(FIELD_USAGE))
}

pub(super) fn facade_override(attribute: &Attribute) -> Result<Path> {
    let mut path = None;
    attribute.parse_nested_meta(|nested| {
        if !nested.path.is_ident("crate") || path.is_some() {
            return Err(nested.error("enum metadata only supports #[meta(crate = path)]; classifications belong on variants"));
        }
        path = Some(nested.value()?.parse()?);
        Ok(())
    })?;
    path.ok_or_else(|| Error::new_spanned(attribute, "expected #[meta(crate = path)]"))
}
