mod attributes;

use syn::{
    Attribute, Data, DeriveInput, Error, Expr, Fields, Generics, Ident, Index, Member, Path,
    Result, Type, Variant, spanned::Spanned,
};

pub(super) struct ErrorMetaInput<'a> {
    pub(super) ident: &'a Ident,
    pub(super) generics: &'a Generics,
    pub(super) facade: Path,
    pub(super) variants: Vec<ClassifiedVariant<'a>>,
}

pub(super) struct ClassifiedVariant<'a> {
    pub(super) ident: &'a Ident,
    pub(super) classification: VariantClassification<'a>,
}

pub(super) enum VariantClassification<'a> {
    Failure(DelegatedField<'a>),
    FailureExpression(Box<Expr>),
    Transparent(DelegatedField<'a>),
    Private(Ident),
}

pub(super) struct DelegatedField<'a> {
    pub(super) member: Member,
    pub(super) field_type: &'a Type,
}

enum VariantMeta {
    Failure(Option<Member>),
    FailureExpression(Box<Expr>),
    Transparent(Option<Member>),
    Private(Ident),
}

impl<'a> ErrorMetaInput<'a> {
    pub(super) fn parse(input: &'a DeriveInput) -> Result<Self> {
        let Data::Enum(data) = &input.data else {
            return Err(Error::new(
                input.ident.span(),
                "ErrorMeta can only be derived for enums",
            ));
        };

        let mut errors = None;
        let mut facade_override = None;
        for attribute in metadata_attributes(&input.attrs) {
            if facade_override.is_some() {
                combine_error(
                    &mut errors,
                    Error::new_spanned(
                        attribute,
                        "each enum takes at most one #[meta(crate = path)] attribute",
                    ),
                );
                continue;
            }
            match attributes::facade_override(attribute) {
                Ok(path) => facade_override = Some(path),
                Err(error) => combine_error(&mut errors, error),
            }
        }

        let mut variants = Vec::with_capacity(data.variants.len());
        for variant in &data.variants {
            for attribute in variant
                .fields
                .iter()
                .flat_map(|field| metadata_attributes(&field.attrs))
            {
                combine_error(
                    &mut errors,
                    Error::new_spanned(
                        attribute,
                        "classification attributes belong on enum variants, not fields",
                    ),
                );
            }
            match ClassifiedVariant::parse(variant) {
                Ok(variant) => variants.push(variant),
                Err(error) => combine_error(&mut errors, error),
            }
        }
        if let Some(errors) = errors {
            return Err(errors);
        }

        Ok(Self {
            ident: &input.ident,
            generics: &input.generics,
            facade: super::facade::resolve(facade_override, input.ident.span())?,
            variants,
        })
    }
}

impl<'a> ClassifiedVariant<'a> {
    fn parse(variant: &'a Variant) -> Result<Self> {
        let attribute = classification_attribute(variant)?;
        let classification = match VariantMeta::parse(attribute)? {
            VariantMeta::Failure(selection) => VariantClassification::Failure(
                DelegatedField::parse(variant, "failure", selection)?,
            ),
            VariantMeta::FailureExpression(reason) => {
                VariantClassification::FailureExpression(reason)
            }
            VariantMeta::Transparent(selection) => VariantClassification::Transparent(
                DelegatedField::parse(variant, "transparent", selection)?,
            ),
            VariantMeta::Private(class) => VariantClassification::Private(class),
        };
        Ok(Self {
            ident: &variant.ident,
            classification,
        })
    }
}

impl<'a> DelegatedField<'a> {
    fn parse(variant: &'a Variant, meta: &str, selection: Option<Member>) -> Result<Self> {
        let Some(member) = selection else {
            let mut fields = variant.fields.iter();
            return match (fields.next(), fields.next()) {
                (Some(field), None) => Ok(Self {
                    member: match &field.ident {
                        Some(ident) => Member::Named(ident.clone()),
                        None => Member::Unnamed(Index::from(0)),
                    },
                    field_type: &field.ty,
                }),
                _ => Err(Error::new(
                    variant.ident.span(),
                    format!("`#[meta({meta})]` requires a variant with exactly one field"),
                )),
            };
        };

        let field = match (&variant.fields, &member) {
            (Fields::Named(fields), Member::Named(name)) => fields
                .named
                .iter()
                .find(|field| field.ident.as_ref() == Some(name)),
            (Fields::Unnamed(fields), Member::Unnamed(index)) => usize::try_from(index.index)
                .ok()
                .and_then(|index| fields.unnamed.iter().nth(index)),
            _ => None,
        };
        let Some(field) = field else {
            return Err(Error::new(
                member.span(),
                format!(
                    "variant `{}` has no {}",
                    variant.ident,
                    member_description(&member)
                ),
            ));
        };
        Ok(Self {
            member,
            field_type: &field.ty,
        })
    }
}

fn member_description(member: &Member) -> String {
    match member {
        Member::Named(name) => format!("named field `{name}`"),
        Member::Unnamed(index) => format!("tuple field at index {}", index.index),
    }
}

fn classification_attribute(variant: &Variant) -> Result<&Attribute> {
    let mut attributes = metadata_attributes(&variant.attrs);
    let Some(attribute) = attributes.next() else {
        return Err(Error::new(
            variant.ident.span(),
            format!(
                "variant `{}` needs a classification: {}",
                variant.ident,
                attributes::META_USAGE
            ),
        ));
    };
    if let Some(duplicate) = attributes.next() {
        return Err(Error::new_spanned(
            duplicate,
            "each variant takes exactly one #[meta(...)] attribute",
        ));
    }
    Ok(attribute)
}

fn metadata_attributes(attributes: &[Attribute]) -> impl Iterator<Item = &Attribute> {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("meta"))
}

fn combine_error(errors: &mut Option<Error>, error: Error) {
    match errors {
        Some(errors) => errors.combine(error),
        None => *errors = Some(error),
    }
}
