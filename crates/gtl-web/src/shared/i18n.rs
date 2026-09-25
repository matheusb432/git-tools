//! Localizes viewer copy through the Fluent catalogs in `crates/gtl-web/i18n`.
//!
//! [`t!`] formats one message in a [`ViewerLanguage`]. The `fl!` macro it wraps
//! checks message IDs and argument names against the en-US catalog at compile
//! time; messages missing from another language fall back to en-US.

use std::{borrow::Cow, sync::LazyLock};

use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use i18n_embed::{
    I18nAssets, I18nEmbedError, LanguageLoader, fluent::FluentLanguageLoader,
    unic_langid::LanguageIdentifier,
};
use unic_langid::langid;

/// Matches the `domain` in `crates/gtl-web/i18n.toml`, which `fl!` checks against.
const CATALOG_DOMAIN: &str = "gtl_web";

/// Every catalog, keyed by the `{language}/{domain}.ftl` path the loader requests.
const CATALOG_FILES: [(&str, &str); 2] = [
    (
        "en-US/gtl_web.ftl",
        include_str!("../../i18n/en-US/gtl_web.ftl"),
    ),
    (
        "pt-BR/gtl_web.ftl",
        include_str!("../../i18n/pt-BR/gtl_web.ftl"),
    ),
];

/// Serves the catalogs compiled into the binary, so no build reads files at runtime.
struct EmbeddedCatalogs;

impl I18nAssets for EmbeddedCatalogs {
    fn get_files(&self, file_path: &str) -> Vec<Cow<'_, [u8]>> {
        CATALOG_FILES
            .iter()
            .filter(|(path, _)| *path == file_path)
            .map(|(_, contents)| Cow::Borrowed(contents.as_bytes()))
            .collect()
    }

    fn filenames_iter(&self) -> Box<dyn Iterator<Item = String> + '_> {
        Box::new(CATALOG_FILES.iter().map(|(path, _)| (*path).to_owned()))
    }
}

const fn language_identifier(language: ViewerLanguage) -> LanguageIdentifier {
    match language {
        ViewerLanguage::EnUs => langid!("en-US"),
        ViewerLanguage::PtBr => langid!("pt-BR"),
    }
}

fn load_catalogs() -> Result<FluentLanguageLoader, I18nEmbedError> {
    let catalogs = FluentLanguageLoader::new(
        CATALOG_DOMAIN,
        language_identifier(ViewerLanguage::default()),
    );
    let languages = ViewerLanguage::ALL
        .iter()
        .copied()
        .map(language_identifier)
        .collect::<Vec<_>>();
    catalogs.load_languages(&EmbeddedCatalogs, &languages)?;
    // Fluent's bidirectional isolation marks would leak into copied text; both
    // supported languages are left-to-right.
    catalogs.set_use_isolating(false);
    Ok(catalogs)
}

/// Every language loaded once. An embedded catalog cannot go missing, and the
/// catalog tests load the same files, so the empty fallback is unreachable in practice.
static CATALOGS: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    load_catalogs().unwrap_or_else(|_| {
        FluentLanguageLoader::new(
            CATALOG_DOMAIN,
            language_identifier(ViewerLanguage::default()),
        )
    })
});

static ENGLISH_CATALOG: LazyLock<FluentLanguageLoader> =
    LazyLock::new(|| CATALOGS.select_languages(&[language_identifier(ViewerLanguage::EnUs)]));

static PORTUGUESE_CATALOG: LazyLock<FluentLanguageLoader> =
    LazyLock::new(|| CATALOGS.select_languages(&[language_identifier(ViewerLanguage::PtBr)]));

/// Returns the catalog that resolves messages in `language`, falling back to en-US.
pub(crate) fn catalog(language: ViewerLanguage) -> &'static FluentLanguageLoader {
    match language {
        ViewerLanguage::EnUs => &ENGLISH_CATALOG,
        ViewerLanguage::PtBr => &PORTUGUESE_CATALOG,
    }
}

/// Formats the catalog message `$message_id` in `$language`.
///
/// Pass arguments as `name = value` pairs. `fl!` rejects an unknown message ID,
/// an unknown argument, or a missing argument against the en-US catalog.
macro_rules! t {
    ($language:expr, $message_id:literal $(, $argument:ident = $value:expr)* $(,)?) => {
        ::i18n_embed_fl::fl!(
            $crate::shared::i18n::catalog($language),
            $message_id
            $(, $argument = ($value))*
        )
    };
}
pub(crate) use t;

/// Names `language` in itself, so readers find their own language whatever the display language.
#[cfg(feature = "interactive-ui")]
pub(crate) const fn language_endonym(language: ViewerLanguage) -> &'static str {
    match language {
        ViewerLanguage::EnUs => "English (US)",
        ViewerLanguage::PtBr => "Português (Brasil)",
    }
}

#[derive(Clone, Copy)]
struct ViewerLanguageContext(ReadSignal<ViewerLanguage>);

/// Makes `language` the language of every descendant's [`use_language`].
pub(crate) fn use_language_provider(language: ReadSignal<ViewerLanguage>) {
    use_context_provider(|| ViewerLanguageContext(language));
}

/// Returns the current language and subscribes the calling component to changes.
///
/// Trees without a provider, such as isolated component tests, use the default language.
pub(crate) fn use_language() -> ViewerLanguage {
    try_use_context::<ViewerLanguageContext>()
        .map_or_else(ViewerLanguage::default, |context| (context.0)())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use fluent_syntax::{
        ast::{Entry, Expression, InlineExpression, Message, Pattern, PatternElement},
        parser,
    };

    use super::*;

    /// Maps each message and attribute of one catalog to the variables it references.
    fn catalog_arguments(source: &'static str) -> BTreeMap<String, BTreeSet<String>> {
        let resource = parser::parse(source).unwrap();
        resource
            .body
            .iter()
            .filter_map(|entry| match entry {
                Entry::Message(message) => Some(message),
                _ => None,
            })
            .flat_map(message_patterns)
            .map(|(id, pattern)| (id, pattern_variables(pattern)))
            .collect()
    }

    fn message_patterns<'message>(
        message: &'message Message<&'static str>,
    ) -> Vec<(String, &'message Pattern<&'static str>)> {
        let id = message.id.name;
        message
            .value
            .iter()
            .map(|value| (id.to_owned(), value))
            .chain(
                message
                    .attributes
                    .iter()
                    .map(|attribute| (format!("{id}.{}", attribute.id.name), &attribute.value)),
            )
            .collect()
    }

    fn pattern_variables(pattern: &Pattern<&'static str>) -> BTreeSet<String> {
        let mut variables = BTreeSet::new();
        pattern
            .elements
            .iter()
            .filter_map(|element| match element {
                PatternElement::Placeable { expression } => Some(expression),
                PatternElement::TextElement { .. } => None,
            })
            .for_each(|expression| expression_variables(expression, &mut variables));
        variables
    }

    fn expression_variables(
        expression: &Expression<&'static str>,
        variables: &mut BTreeSet<String>,
    ) {
        match expression {
            Expression::Inline(inline) => inline_variables(inline, variables),
            Expression::Select { selector, variants } => {
                inline_variables(selector, variables);
                variables.extend(
                    variants
                        .iter()
                        .flat_map(|variant| pattern_variables(&variant.value)),
                );
            }
        }
    }

    fn inline_variables(
        expression: &InlineExpression<&'static str>,
        variables: &mut BTreeSet<String>,
    ) {
        match expression {
            InlineExpression::VariableReference { id } => {
                variables.insert(id.name.to_owned());
            }
            InlineExpression::Placeable { expression } => {
                expression_variables(expression, variables);
            }
            _ => {}
        }
    }

    #[test]
    fn every_language_defines_the_english_messages_with_the_same_arguments() {
        let [(_, english), (_, portuguese)] = CATALOG_FILES;

        assert_eq!(catalog_arguments(portuguese), catalog_arguments(english));
    }

    #[test]
    fn each_language_formats_messages_from_its_own_catalog() {
        assert_eq!(t!(ViewerLanguage::EnUs, "settings-save"), "Save settings");
        assert_eq!(
            t!(ViewerLanguage::PtBr, "settings-save"),
            "Salvar configurações"
        );
        assert_eq!(
            t!(
                ViewerLanguage::PtBr,
                "connection-retrying",
                message = "Falhou."
            ),
            "Falhou. Tentando novamente automaticamente."
        );
    }

    #[test]
    fn every_catalog_loads_without_errors() {
        let invalid = CATALOG_FILES
            .iter()
            .filter(|(_, source)| parser::parse(*source).is_err())
            .map(|(path, _)| *path)
            .collect::<Vec<_>>();

        assert_eq!(invalid, Vec::<&str>::new());
        assert!(load_catalogs().is_ok());
    }
}
