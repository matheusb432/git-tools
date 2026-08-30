use syn::{Expr, Lit, LitStr, MetaNameValue};

use crate::{diagnostics, naming};

#[derive(Default)]
pub(crate) struct MetadataArguments {
    pub(crate) id: Option<LitStr>,
    pub(crate) name: Option<LitStr>,
}

impl MetadataArguments {
    pub(crate) fn push(
        &mut self,
        argument: &MetaNameValue,
        errors: &mut Option<syn::Error>,
    ) -> bool {
        let (target, name) = if argument.path.is_ident("id") {
            (&mut self.id, "id")
        } else if argument.path.is_ident("name") {
            (&mut self.name, "name")
        } else {
            return false;
        };

        if target.is_some() {
            diagnostics::combine(
                errors,
                syn::Error::new_spanned(&argument.value, format!("duplicate `{name}` argument")),
            );
            return true;
        }

        if let Expr::Lit(expression) = &argument.value
            && let Lit::Str(value) = &expression.lit
        {
            *target = Some(value.clone());
        } else {
            diagnostics::combine(
                errors,
                syn::Error::new_spanned(
                    &argument.value,
                    format!("`{name}` must be a string literal"),
                ),
            );
        }
        true
    }

    pub(crate) fn validate(&self, kind: &str, errors: &mut Option<syn::Error>) {
        if let Some(id) = &self.id
            && let Err(error) = naming::validate_id(&id.value(), kind, id.span())
        {
            diagnostics::combine(errors, error);
        }
        if let Some(name) = &self.name
            && let Err(error) = naming::validate_name(&name.value(), kind, name.span())
        {
            diagnostics::combine(errors, error);
        }
    }
}
