pub(crate) fn combine(errors: &mut Option<syn::Error>, error: syn::Error) {
    if let Some(errors) = errors {
        errors.combine(error);
    } else {
        *errors = Some(error);
    }
}

pub(crate) fn capture<T>(errors: &mut Option<syn::Error>, result: syn::Result<T>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            combine(errors, error);
            None
        }
    }
}

pub(crate) fn finish(errors: Option<syn::Error>) -> syn::Result<()> {
    errors.map_or(Ok(()), Err)
}

pub(crate) fn validated<T>(
    value: Option<T>,
    label: &str,
    span: proc_macro2::Span,
) -> syn::Result<T> {
    value.ok_or_else(|| {
        syn::Error::new(
            span,
            format!("internal storybook error: validated {label} is unavailable"),
        )
    })
}
