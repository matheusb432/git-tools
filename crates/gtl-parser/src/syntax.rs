use std::{
    cell::RefCell,
    collections::HashMap,
    path::Path,
    sync::{LazyLock, OnceLock},
};

use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};
use unicase::Ascii;

use crate::SyntaxTokenClass;

static SYNTAX_LANGUAGES_BY_EXTENSION: LazyLock<HashMap<Ascii<&'static str>, SyntaxLanguage>> =
    LazyLock::new(|| {
        HashMap::from([
            (Ascii::new("js"), SyntaxLanguage::JavaScript),
            (Ascii::new("ts"), SyntaxLanguage::TypeScript),
            (Ascii::new("py"), SyntaxLanguage::Python),
            (Ascii::new("rs"), SyntaxLanguage::Rust),
            (Ascii::new("md"), SyntaxLanguage::Markdown),
            (Ascii::new("html"), SyntaxLanguage::Html),
            (Ascii::new("yml"), SyntaxLanguage::Yaml),
            (Ascii::new("yaml"), SyntaxLanguage::Yaml),
            (Ascii::new("toml"), SyntaxLanguage::Toml),
            (Ascii::new("h"), SyntaxLanguage::C),
            (Ascii::new("c"), SyntaxLanguage::C),
            (Ascii::new("cpp"), SyntaxLanguage::Cpp),
            (Ascii::new("swift"), SyntaxLanguage::Swift),
            (Ascii::new("cs"), SyntaxLanguage::CSharp),
        ])
    });

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
    Toml,
    C,
    Cpp,
    Swift,
    CSharp,
}

impl SyntaxLanguage {
    /// Resolves a supported file extension without consulting the filesystem.
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        SYNTAX_LANGUAGES_BY_EXTENSION
            .get(&Ascii::new(extension))
            .copied()
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

        let configurations = syntax_configurations();
        let configuration = configurations.for_language(language)?;
        let injection_error = RefCell::new(None);
        let events = self
            .highlighter
            .highlight(
                configuration,
                source.as_bytes(),
                None,
                |injection| match configurations.for_injection(injection) {
                    Ok(configuration) => configuration,
                    Err(error) => {
                        injection_error.borrow_mut().get_or_insert(error);
                        None
                    }
                },
            )
            .map_err(|error| error.to_string())?;
        let mut classes = Vec::new();
        let mut tokens = Vec::new();

        for event in events {
            observe_highlight_event(
                event.map_err(|error| error.to_string())?,
                &mut classes,
                &mut tokens,
            )?;
        }

        if !classes.is_empty() {
            return Err("highlight event stream ended with open captures".to_owned());
        }
        if let Some(error) = injection_error.into_inner() {
            return Err(error);
        }
        Ok(tokens)
    }
}

fn observe_highlight_event(
    event: HighlightEvent,
    classes: &mut Vec<SyntaxTokenClass>,
    tokens: &mut Vec<ByteSyntaxToken>,
) -> Result<(), String> {
    match event {
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
                push_byte_token(tokens, start, end, class);
            }
        }
    }
    Ok(())
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
    javascript: OnceLock<Result<HighlightConfiguration, String>>,
    typescript: OnceLock<Result<HighlightConfiguration, String>>,
    python: OnceLock<Result<HighlightConfiguration, String>>,
    rust: OnceLock<Result<HighlightConfiguration, String>>,
    markdown: OnceLock<Result<HighlightConfiguration, String>>,
    markdown_inline: OnceLock<Result<HighlightConfiguration, String>>,
    html: OnceLock<Result<HighlightConfiguration, String>>,
    yaml: OnceLock<Result<HighlightConfiguration, String>>,
    toml: OnceLock<Result<HighlightConfiguration, String>>,
    c: OnceLock<Result<HighlightConfiguration, String>>,
    cpp: OnceLock<Result<HighlightConfiguration, String>>,
    swift: OnceLock<Result<HighlightConfiguration, String>>,
    c_sharp: OnceLock<Result<HighlightConfiguration, String>>,
}

impl SyntaxConfigurations {
    const fn new() -> Self {
        Self {
            javascript: OnceLock::new(),
            typescript: OnceLock::new(),
            python: OnceLock::new(),
            rust: OnceLock::new(),
            markdown: OnceLock::new(),
            markdown_inline: OnceLock::new(),
            html: OnceLock::new(),
            yaml: OnceLock::new(),
            toml: OnceLock::new(),
            c: OnceLock::new(),
            cpp: OnceLock::new(),
            swift: OnceLock::new(),
            c_sharp: OnceLock::new(),
        }
    }

    fn for_language(&self, language: SyntaxLanguage) -> Result<&HighlightConfiguration, String> {
        let configuration = match language {
            SyntaxLanguage::JavaScript => self.javascript.get_or_init(javascript_configuration),
            SyntaxLanguage::TypeScript => self.typescript.get_or_init(typescript_configuration),
            SyntaxLanguage::Python => self.python.get_or_init(python_configuration),
            SyntaxLanguage::Rust => self.rust.get_or_init(rust_configuration),
            SyntaxLanguage::Markdown => self.markdown.get_or_init(markdown_configuration),
            SyntaxLanguage::Html => self.html.get_or_init(html_configuration),
            SyntaxLanguage::Yaml => self.yaml.get_or_init(yaml_configuration),
            SyntaxLanguage::Toml => self.toml.get_or_init(toml_configuration),
            SyntaxLanguage::C => self.c.get_or_init(c_configuration),
            SyntaxLanguage::Cpp => self.cpp.get_or_init(cpp_configuration),
            SyntaxLanguage::Swift => self.swift.get_or_init(swift_configuration),
            SyntaxLanguage::CSharp => self.c_sharp.get_or_init(c_sharp_configuration),
        };
        configuration.as_ref().map_err(Clone::clone)
    }

    fn for_injection(&self, name: &str) -> Result<Option<&HighlightConfiguration>, String> {
        if name.eq_ignore_ascii_case("javascript") || name.eq_ignore_ascii_case("js") {
            self.for_language(SyntaxLanguage::JavaScript).map(Some)
        } else if name.eq_ignore_ascii_case("typescript") || name.eq_ignore_ascii_case("ts") {
            self.for_language(SyntaxLanguage::TypeScript).map(Some)
        } else if name.eq_ignore_ascii_case("python") || name.eq_ignore_ascii_case("py") {
            self.for_language(SyntaxLanguage::Python).map(Some)
        } else if name.eq_ignore_ascii_case("rust") || name.eq_ignore_ascii_case("rs") {
            self.for_language(SyntaxLanguage::Rust).map(Some)
        } else if name.eq_ignore_ascii_case("markdown") || name.eq_ignore_ascii_case("md") {
            self.for_language(SyntaxLanguage::Markdown).map(Some)
        } else if name.eq_ignore_ascii_case("markdown_inline") {
            self.markdown_inline
                .get_or_init(markdown_inline_configuration)
                .as_ref()
                .map(Some)
                .map_err(Clone::clone)
        } else if name.eq_ignore_ascii_case("html") {
            self.for_language(SyntaxLanguage::Html).map(Some)
        } else if name.eq_ignore_ascii_case("yaml") || name.eq_ignore_ascii_case("yml") {
            self.for_language(SyntaxLanguage::Yaml).map(Some)
        } else if name.eq_ignore_ascii_case("toml") {
            self.for_language(SyntaxLanguage::Toml).map(Some)
        } else if name.eq_ignore_ascii_case("c") {
            self.for_language(SyntaxLanguage::C).map(Some)
        } else if name.eq_ignore_ascii_case("cpp") || name.eq_ignore_ascii_case("c++") {
            self.for_language(SyntaxLanguage::Cpp).map(Some)
        } else if name.eq_ignore_ascii_case("swift") {
            self.for_language(SyntaxLanguage::Swift).map(Some)
        } else if name.eq_ignore_ascii_case("c_sharp")
            || name.eq_ignore_ascii_case("csharp")
            || name.eq_ignore_ascii_case("c#")
            || name.eq_ignore_ascii_case("cs")
        {
            self.for_language(SyntaxLanguage::CSharp).map(Some)
        } else {
            Ok(None)
        }
    }
}

fn javascript_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_javascript::LANGUAGE.into(),
        "javascript",
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_javascript::INJECTIONS_QUERY,
        tree_sitter_javascript::LOCALS_QUERY,
    )
}

fn typescript_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "typescript",
        tree_sitter_typescript::HIGHLIGHTS_QUERY,
        "",
        tree_sitter_typescript::LOCALS_QUERY,
    )
}

fn python_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_python::LANGUAGE.into(),
        "python",
        tree_sitter_python::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

fn rust_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_rust::LANGUAGE.into(),
        "rust",
        tree_sitter_rust::HIGHLIGHTS_QUERY,
        tree_sitter_rust::INJECTIONS_QUERY,
        "",
    )
}

fn markdown_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_md::LANGUAGE.into(),
        "markdown",
        tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
        tree_sitter_md::INJECTION_QUERY_BLOCK,
        "",
    )
}

fn markdown_inline_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_md::INLINE_LANGUAGE.into(),
        "markdown_inline",
        tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
        tree_sitter_md::INJECTION_QUERY_INLINE,
        "",
    )
}

fn html_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_html::LANGUAGE.into(),
        "html",
        tree_sitter_html::HIGHLIGHTS_QUERY,
        tree_sitter_html::INJECTIONS_QUERY,
        "",
    )
}

fn yaml_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_yaml::LANGUAGE.into(),
        "yaml",
        tree_sitter_yaml::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

fn toml_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_toml_ng::LANGUAGE.into(),
        "toml",
        tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

fn c_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_c::LANGUAGE.into(),
        "c",
        tree_sitter_c::HIGHLIGHT_QUERY,
        "",
        "",
    )
}

fn cpp_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_cpp::LANGUAGE.into(),
        "cpp",
        tree_sitter_cpp::HIGHLIGHT_QUERY,
        "",
        "",
    )
}

fn swift_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_swift::LANGUAGE.into(),
        "swift",
        tree_sitter_swift::HIGHLIGHTS_QUERY,
        tree_sitter_swift::INJECTIONS_QUERY,
        tree_sitter_swift::LOCALS_QUERY,
    )
}

fn c_sharp_configuration() -> Result<HighlightConfiguration, String> {
    configuration(
        tree_sitter_c_sharp::LANGUAGE.into(),
        "c_sharp",
        tree_sitter_c_sharp::HIGHLIGHTS_QUERY,
        "",
        "",
    )
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

fn syntax_configurations() -> &'static SyntaxConfigurations {
    static CONFIGURATIONS: SyntaxConfigurations = SyntaxConfigurations::new();
    &CONFIGURATIONS
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
            ("a.ToMl", SyntaxLanguage::Toml),
            ("a.H", SyntaxLanguage::C),
            ("a.C", SyntaxLanguage::C),
            ("a.CpP", SyntaxLanguage::Cpp),
            ("a.SwIfT", SyntaxLanguage::Swift),
            ("a.Cs", SyntaxLanguage::CSharp),
        ] {
            assert_eq!(
                SyntaxLanguage::from_path(Path::new(path)),
                Some(expected),
                "{path}"
            );
        }
    }

    #[test]
    fn unknown_or_missing_extensions_resolve_none() {
        for path in ["file.zzzunknown", "no-extension", ""] {
            assert_eq!(SyntaxLanguage::from_path(Path::new(path)), None);
        }
    }

    #[cfg(unix)]
    #[test]
    fn supported_extension_does_not_require_a_utf8_path() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt as _, path::PathBuf};

        let mut path = PathBuf::from(OsString::from_vec(vec![b'd', 0x80]));
        path.push("file.RS");

        assert_eq!(SyntaxLanguage::from_path(&path), Some(SyntaxLanguage::Rust));
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
            (SyntaxLanguage::Toml, "key = 1"),
            (SyntaxLanguage::C, "int value = 1;"),
            (SyntaxLanguage::Cpp, "auto value = 1;"),
            (SyntaxLanguage::Swift, "let value = 1"),
            (SyntaxLanguage::CSharp, "var value = 1;"),
        ] {
            let tokens = SideHighlighter::new().tokens(language, source).unwrap();
            assert!(!tokens.is_empty(), "no tokens for {language:?}");
        }
    }

    #[test]
    fn configuration_cache_initializes_only_the_requested_language() {
        let configurations = SyntaxConfigurations::new();

        assert!(configurations.rust.get().is_none());
        configurations.for_language(SyntaxLanguage::Rust).unwrap();

        assert!(configurations.rust.get().is_some());
        assert!(configurations.javascript.get().is_none());
        assert!(configurations.typescript.get().is_none());
        assert!(configurations.python.get().is_none());
        assert!(configurations.markdown.get().is_none());
        assert!(configurations.markdown_inline.get().is_none());
        assert!(configurations.html.get().is_none());
        assert!(configurations.yaml.get().is_none());
        assert!(configurations.toml.get().is_none());
        assert!(configurations.c.get().is_none());
        assert!(configurations.cpp.get().is_none());
        assert!(configurations.swift.get().is_none());
        assert!(configurations.c_sharp.get().is_none());
    }

    #[test]
    fn markdown_inline_configuration_waits_for_an_injection() {
        let configurations = SyntaxConfigurations::new();

        configurations
            .for_language(SyntaxLanguage::Markdown)
            .unwrap();
        assert!(configurations.markdown.get().is_some());
        assert!(configurations.markdown_inline.get().is_none());

        assert!(
            configurations
                .for_injection("markdown_inline")
                .unwrap()
                .is_some()
        );
        assert!(configurations.markdown_inline.get().is_some());
    }
}
