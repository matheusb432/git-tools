use proc_macro2::{Ident, Span};
use quote::format_ident;
use syn::ext::IdentExt;

const VARIANT_PREFIX: &str = "VARIANT_";

pub(crate) fn story_const_ident(ident: &Ident) -> Ident {
    generated_uppercase_ident("STORY_", ident)
}

pub(crate) fn variant_const_ident(ident: &Ident) -> Ident {
    generated_uppercase_ident(VARIANT_PREFIX, ident)
}

pub(crate) fn variant_component_ident(ident: &Ident) -> Ident {
    let suffix = variant_title(ident, "");
    format_ident!("DioxusStorybookVariant{suffix}", span = ident.span())
}

pub(crate) fn variant_id(ident: &Ident) -> syn::Result<String> {
    let name = ident.unraw().to_string();
    let valid = name
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && name
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && !name.contains("__");

    if !valid {
        return Err(syn::Error::new(
            ident.span(),
            "variant function names must be canonical lowercase ASCII snake_case",
        ));
    }

    Ok(name.replace('_', "-"))
}

pub(crate) fn variant_name(ident: &Ident) -> String {
    variant_title(ident, " ")
}

fn variant_title(ident: &Ident, separator: &str) -> String {
    ident
        .unraw()
        .to_string()
        .split('_')
        .map(capitalize)
        .collect::<Vec<_>>()
        .join(separator)
}

pub(crate) fn validate_id(id: &str, kind: &str, span: Span) -> syn::Result<()> {
    let bytes = id.as_bytes();
    let has_valid_edges = bytes.first().is_some_and(u8::is_ascii_lowercase)
        && bytes
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    let mut previous_was_separator = false;
    let has_valid_contents = bytes.iter().all(|byte| {
        let separator = matches!(byte, b'-' | b'_');
        let valid = byte.is_ascii_lowercase() || byte.is_ascii_digit() || separator;
        let canonical = valid && !(separator && previous_was_separator);
        previous_was_separator = separator;
        canonical
    });
    if has_valid_edges && has_valid_contents {
        return Ok(());
    }

    Err(syn::Error::new(
        span,
        format!(
            "{kind} `id` must contain lowercase ASCII letters, digits, and single internal hyphens or underscores"
        ),
    ))
}

pub(crate) fn validate_name(name: &str, kind: &str, span: Span) -> syn::Result<()> {
    if !name.is_empty() && name.trim() == name {
        return Ok(());
    }
    Err(syn::Error::new(
        span,
        format!("{kind} `name` must be non-empty and have no surrounding whitespace"),
    ))
}

fn generated_uppercase_ident(prefix: &str, ident: &Ident) -> Ident {
    let suffix = ident.unraw().to_string().to_ascii_uppercase();
    format_ident!("{prefix}{suffix}", span = ident.span())
}

fn capitalize(word: &str) -> String {
    let mut characters = word.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };

    first.to_uppercase().chain(characters).collect()
}
