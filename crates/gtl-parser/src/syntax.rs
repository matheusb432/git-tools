use std::{path::Path, sync::LazyLock};

use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

use crate::SyntaxTokenClass;

/// A syntax grammar compiled into the `syntax` feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxLanguage {
    JavaScript,
    TypeScript,
    Python,
    Rust,
    Markdown,
    Html,
    Yaml,
}

impl SyntaxLanguage {
    /// Resolves a supported file extension without consulting the filesystem.
    pub fn from_path(path: &str) -> Option<Self> {
        let extension = Path::new(path).extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("js") {
            Some(Self::JavaScript)
        } else if extension.eq_ignore_ascii_case("ts") {
            Some(Self::TypeScript)
        } else if extension.eq_ignore_ascii_case("py") {
            Some(Self::Python)
        } else if extension.eq_ignore_ascii_case("rs") {
            Some(Self::Rust)
        } else if extension.eq_ignore_ascii_case("md") {
            Some(Self::Markdown)
        } else if extension.eq_ignore_ascii_case("html") {
            Some(Self::Html)
        } else if extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml") {
            Some(Self::Yaml)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ByteSyntaxToken {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) class: SyntaxTokenClass,
}

pub(crate) struct SideHighlighter {
    highlighter: Highlighter,
}

impl SideHighlighter {
    pub(crate) fn new() -> Self {
        Self {
            highlighter: Highlighter::new(),
        }
    }

    pub(crate) fn tokens(
        &mut self,
        language: SyntaxLanguage,
        source: &str,
    ) -> Result<Vec<ByteSyntaxToken>, String> {
        if source.is_empty() {
            return Ok(Vec::new());
        }

        let configurations = syntax_configurations()?;
        let configuration = configurations.for_language(language);
        let events = self
            .highlighter
            .highlight(configuration, source.as_bytes(), None, |injection| {
                configurations.for_injection(injection)
            })
            .map_err(|error| error.to_string())?;
        let mut classes = Vec::new();
        let mut tokens = Vec::new();

        for event in events {
            match event.map_err(|error| error.to_string())? {
                HighlightEvent::HighlightStart(highlight) => {
                    let Some((_, class)) = HIGHLIGHT_CLASSES.get(highlight.0) else {
                        return Err(format!("highlight index {} is out of bounds", highlight.0));
                    };
                    classes.push(*class);
                }
                HighlightEvent::HighlightEnd => {
                    if classes.pop().is_none() {
                        return Err("highlight event ended without a matching start".to_owned());
                    }
                }
                HighlightEvent::Source { start, end } => {
                    if let Some(class) = classes.last().copied() {
                        push_byte_token(&mut tokens, start, end, class);
                    }
                }
            }
        }

        if !classes.is_empty() {
            return Err("highlight event stream ended with open captures".to_owned());
        }
        Ok(tokens)
    }
}

fn push_byte_token(
    tokens: &mut Vec<ByteSyntaxToken>,
    start: usize,
    end: usize,
    class: SyntaxTokenClass,
) {
    if start == end {
        return;
    }
    match tokens.last_mut() {
        Some(last) if last.end == start && last.class == class => last.end = end,
        _ => tokens.push(ByteSyntaxToken { start, end, class }),
    }
}

struct SyntaxConfigurations {
    javascript: HighlightConfiguration,
    typescript: HighlightConfiguration,
    python: HighlightConfiguration,
    rust: HighlightConfiguration,
    markdown: HighlightConfiguration,
    markdown_inline: HighlightConfiguration,
    html: HighlightConfiguration,
    yaml: HighlightConfiguration,
}

impl SyntaxConfigurations {
    fn new() -> Result<Self, String> {
        Ok(Self {
            javascript: configuration(
                tree_sitter_javascript::LANGUAGE.into(),
                "javascript",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::INJECTIONS_QUERY,
                tree_sitter_javascript::LOCALS_QUERY,
            )?,
            typescript: configuration(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                "typescript",
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
                "",
                tree_sitter_typescript::LOCALS_QUERY,
            )?,
            python: configuration(
                tree_sitter_python::LANGUAGE.into(),
                "python",
                tree_sitter_python::HIGHLIGHTS_QUERY,
                "",
                "",
            )?,
            rust: configuration(
                tree_sitter_rust::LANGUAGE.into(),
                "rust",
                tree_sitter_rust::HIGHLIGHTS_QUERY,
                tree_sitter_rust::INJECTIONS_QUERY,
                "",
            )?,
            markdown: configuration(
                tree_sitter_md::LANGUAGE.into(),
                "markdown",
                tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
                tree_sitter_md::INJECTION_QUERY_BLOCK,
                "",
            )?,
            markdown_inline: configuration(
                tree_sitter_md::INLINE_LANGUAGE.into(),
                "markdown_inline",
                tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
                tree_sitter_md::INJECTION_QUERY_INLINE,
                "",
            )?,
            html: configuration(
                tree_sitter_html::LANGUAGE.into(),
                "html",
                tree_sitter_html::HIGHLIGHTS_QUERY,
                tree_sitter_html::INJECTIONS_QUERY,
                "",
            )?,
            yaml: configuration(
                tree_sitter_yaml::LANGUAGE.into(),
                "yaml",
                tree_sitter_yaml::HIGHLIGHTS_QUERY,
                "",
                "",
            )?,
        })
    }

    const fn for_language(&self, language: SyntaxLanguage) -> &HighlightConfiguration {
        match language {
            SyntaxLanguage::JavaScript => &self.javascript,
            SyntaxLanguage::TypeScript => &self.typescript,
            SyntaxLanguage::Python => &self.python,
            SyntaxLanguage::Rust => &self.rust,
            SyntaxLanguage::Markdown => &self.markdown,
            SyntaxLanguage::Html => &self.html,
            SyntaxLanguage::Yaml => &self.yaml,
        }
    }

    fn for_injection(&self, name: &str) -> Option<&HighlightConfiguration> {
        if name.eq_ignore_ascii_case("javascript") || name.eq_ignore_ascii_case("js") {
            Some(&self.javascript)
        } else if name.eq_ignore_ascii_case("typescript") || name.eq_ignore_ascii_case("ts") {
            Some(&self.typescript)
        } else if name.eq_ignore_ascii_case("python") || name.eq_ignore_ascii_case("py") {
            Some(&self.python)
        } else if name.eq_ignore_ascii_case("rust") || name.eq_ignore_ascii_case("rs") {
            Some(&self.rust)
        } else if name.eq_ignore_ascii_case("markdown") || name.eq_ignore_ascii_case("md") {
            Some(&self.markdown)
        } else if name.eq_ignore_ascii_case("markdown_inline") {
            Some(&self.markdown_inline)
        } else if name.eq_ignore_ascii_case("html") {
            Some(&self.html)
        } else if name.eq_ignore_ascii_case("yaml") || name.eq_ignore_ascii_case("yml") {
            Some(&self.yaml)
        } else {
            None
        }
    }
}

fn configuration(
    language: Language,
    name: &str,
    highlights_query: &str,
    injections_query: &str,
    locals_query: &str,
) -> Result<HighlightConfiguration, String> {
    let mut configuration = HighlightConfiguration::new(
        language,
        name,
        highlights_query,
        injections_query,
        locals_query,
    )
    .map_err(|error| format!("{name} highlight query is invalid: {error}"))?;
    let names = HIGHLIGHT_CLASSES.map(|(name, _)| name);
    configuration.configure(&names);
    Ok(configuration)
}

fn syntax_configurations() -> Result<&'static SyntaxConfigurations, String> {
    static CONFIGURATIONS: LazyLock<Result<SyntaxConfigurations, String>> =
        LazyLock::new(SyntaxConfigurations::new);
    CONFIGURATIONS.as_ref().map_err(Clone::clone)
}

const HIGHLIGHT_CLASSES: [(&str, SyntaxTokenClass); 25] = [
    ("keyword", SyntaxTokenClass::Keyword),
    ("string", SyntaxTokenClass::String),
    ("comment", SyntaxTokenClass::Comment),
    ("type", SyntaxTokenClass::Type),
    ("constructor", SyntaxTokenClass::Type),
    ("function", SyntaxTokenClass::Function),
    ("number", SyntaxTokenClass::Number),
    ("constant", SyntaxTokenClass::Constant),
    ("boolean", SyntaxTokenClass::Constant),
    ("operator", SyntaxTokenClass::Operator),
    ("tag", SyntaxTokenClass::Tag),
    ("markup.heading", SyntaxTokenClass::Tag),
    ("markup.raw", SyntaxTokenClass::String),
    ("markup.bold", SyntaxTokenClass::Keyword),
    ("markup.italic", SyntaxTokenClass::Keyword),
    ("text.title", SyntaxTokenClass::Tag),
    ("text.literal", SyntaxTokenClass::String),
    ("text.strong", SyntaxTokenClass::Keyword),
    ("text.emphasis", SyntaxTokenClass::Keyword),
    ("text.uri", SyntaxTokenClass::String),
    ("text.reference", SyntaxTokenClass::Variable),
    ("variable", SyntaxTokenClass::Variable),
    ("variable.parameter", SyntaxTokenClass::Variable),
    ("attribute", SyntaxTokenClass::Variable),
    ("property", SyntaxTokenClass::Variable),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_extensions_resolve_case_insensitively() {
        for (path, expected) in [
            ("a.JS", SyntaxLanguage::JavaScript),
            ("a.Ts", SyntaxLanguage::TypeScript),
            ("a.PY", SyntaxLanguage::Python),
            ("a.Rs", SyntaxLanguage::Rust),
            ("a.MD", SyntaxLanguage::Markdown),
            ("a.HTML", SyntaxLanguage::Html),
            ("a.YmL", SyntaxLanguage::Yaml),
            ("a.YAML", SyntaxLanguage::Yaml),
        ] {
            assert_eq!(SyntaxLanguage::from_path(path), Some(expected), "{path}");
        }
    }

    #[test]
    fn unknown_or_missing_extensions_resolve_none() {
        for path in ["file.zzzunknown", "no-extension", ""] {
            assert_eq!(SyntaxLanguage::from_path(path), None);
        }
    }

    #[test]
    fn every_language_configuration_produces_highlights() {
        for (language, source) in [
            (SyntaxLanguage::JavaScript, "const value = 1;"),
            (SyntaxLanguage::TypeScript, "const value: number = 1;"),
            (SyntaxLanguage::Python, "value = 1  # note"),
            (SyntaxLanguage::Rust, "let value = 1;"),
            (SyntaxLanguage::Markdown, "# heading with **weight**"),
            (SyntaxLanguage::Html, "<main data-value=\"1\">text</main>"),
            (SyntaxLanguage::Yaml, "key: 1"),
        ] {
            let tokens = SideHighlighter::new()
                .tokens(language, source)
                .expect("upstream query should highlight the fixture");
            assert!(!tokens.is_empty(), "no tokens for {language:?}");
        }
    }
}
