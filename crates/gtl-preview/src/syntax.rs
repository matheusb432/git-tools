use std::sync::Arc;

pub(crate) use gtl_parser::SyntaxDefinition;

/// A bundled syntax-catalog failure encountered while rendering a preview.
pub type PreviewError = gtl_parser::SyntaxCatalogError;
use gtl_parser::{SyntaxTokenClass, bundled_syntax_catalog};

/// The result of rendering preview content that depends on bundled syntax assets.
pub type PreviewResult<T> = Result<T, Arc<PreviewError>>;

/// Resolves a file path against the parser crate's bundled syntax catalog.
pub(crate) fn syntax_for_path(path: &str) -> PreviewResult<Option<SyntaxDefinition>> {
    bundled_syntax_catalog().map(|catalog| catalog.syntax_for_path(path))
}

/// Maps semantic parser tokens to preview-owned presentation classes.
pub(crate) const fn token_css_class(class: SyntaxTokenClass) -> &'static str {
    match class {
        SyntaxTokenClass::Keyword => "sy-kw",
        SyntaxTokenClass::String => "sy-str",
        SyntaxTokenClass::Comment => "sy-com",
        SyntaxTokenClass::Type => "sy-typ",
        SyntaxTokenClass::Function => "sy-fn",
        SyntaxTokenClass::Number => "sy-num",
        SyntaxTokenClass::Constant => "sy-con",
        SyntaxTokenClass::Operator => "sy-op",
        SyntaxTokenClass::Tag => "sy-tag",
        SyntaxTokenClass::Variable => "sy-var",
    }
}
