#[cfg(feature = "bundled-syntaxes")]
use std::sync::LazyLock;
use std::{fmt, path::Path, sync::Arc};

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

use crate::{SyntaxToken, SyntaxTokenClass};

/// A failure while constructing a syntax catalog.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SyntaxCatalogError {
    #[error("syntax pack is invalid: {message}")]
    SyntaxPack { message: String },
    #[error("syntax scope prefix is invalid: {prefix}: {message}")]
    ScopePrefix {
        prefix: &'static str,
        message: String,
    },
}

struct SyntaxAssets {
    syntax_set: SyntaxSet,
    scope_map: Vec<(Scope, SyntaxTokenClass)>,
}

/// An owned, cloneable collection of syntax grammars.
///
/// Clones share immutable grammar data. No `syntect` type appears in the
/// public API.
#[derive(Clone)]
pub struct SyntaxCatalog {
    assets: Arc<SyntaxAssets>,
}

impl fmt::Debug for SyntaxCatalog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SyntaxCatalog")
            .field("syntax_count", &self.assets.syntax_set.syntaxes().len())
            .finish_non_exhaustive()
    }
}

impl SyntaxCatalog {
    /// Loads a catalog from an uncompressed `syntect` syntax-set dump.
    pub fn from_uncompressed_pack(syntax_pack: &[u8]) -> Result<Self, SyntaxCatalogError> {
        let syntax_set = syntect::dumps::from_uncompressed_data(syntax_pack).map_err(|error| {
            SyntaxCatalogError::SyntaxPack {
                message: error.to_string(),
            }
        })?;
        let scope_map = SCOPE_PREFIXES
            .iter()
            .copied()
            .map(|(prefix, class)| {
                Scope::new(prefix)
                    .map(|scope| (scope, class))
                    .map_err(|error| SyntaxCatalogError::ScopePrefix {
                        prefix,
                        message: error.to_string(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            assets: Arc::new(SyntaxAssets {
                syntax_set,
                scope_map,
            }),
        })
    }

    /// Resolves a path's extension to an owned syntax definition.
    pub fn syntax_for_path(&self, path: &str) -> Option<SyntaxDefinition> {
        let extension = Path::new(path).extension()?.to_str()?;
        let syntax = self.assets.syntax_set.find_syntax_by_extension(extension)?;
        let syntax_index = self
            .assets
            .syntax_set
            .syntaxes()
            .iter()
            .position(|candidate| std::ptr::eq(candidate, syntax))?;
        Some(SyntaxDefinition {
            assets: Arc::clone(&self.assets),
            syntax_index,
        })
    }
}

/// An owned syntax selection resolved from a [`SyntaxCatalog`].
#[derive(Clone)]
pub struct SyntaxDefinition {
    assets: Arc<SyntaxAssets>,
    syntax_index: usize,
}

impl fmt::Debug for SyntaxDefinition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SyntaxDefinition")
            .field(
                "name",
                &self.assets.syntax_set.syntaxes()[self.syntax_index].name,
            )
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "bundled-syntaxes")]
static BUNDLED_SYNTAX_CATALOG: LazyLock<Result<SyntaxCatalog, Arc<SyntaxCatalogError>>> =
    LazyLock::new(|| {
        SyntaxCatalog::from_uncompressed_pack(include_bytes!("../assets/syntaxes.packdump"))
            .map_err(Arc::new)
    });

/// Returns the syntax catalog bundled with this crate.
#[cfg(feature = "bundled-syntaxes")]
pub fn bundled_syntax_catalog() -> Result<SyntaxCatalog, Arc<SyntaxCatalogError>> {
    BUNDLED_SYNTAX_CATALOG.clone()
}

pub(crate) struct SideHighlighter {
    parse: ParseState,
    stack: ScopeStack,
    assets: Arc<SyntaxAssets>,
    disabled: bool,
}

impl SideHighlighter {
    pub(crate) fn new(definition: &SyntaxDefinition) -> Self {
        let syntax = &definition.assets.syntax_set.syntaxes()[definition.syntax_index];
        Self {
            parse: ParseState::new(syntax),
            stack: ScopeStack::new(),
            assets: Arc::clone(&definition.assets),
            disabled: false,
        }
    }

    pub(crate) fn tokens(&mut self, body: &str) -> Result<Vec<SyntaxToken>, String> {
        if self.disabled {
            return Ok(Vec::new());
        }

        let line = format!("{body}\n");
        let ops = self
            .parse
            .parse_line(&line, &self.assets.syntax_set)
            .map_err(|error| self.disable(error))?;

        let mut tokens = Vec::new();
        let mut byte_position = 0usize;
        let mut character_position = 0usize;
        for (operation_byte, operation) in ops {
            let clamped = operation_byte.min(body.len());
            if clamped > byte_position {
                let end = character_position + body[byte_position..clamped].chars().count();
                push_token(
                    &mut tokens,
                    character_position,
                    end,
                    class_for(&self.stack, &self.assets.scope_map),
                );
                character_position = end;
                byte_position = clamped;
            }
            if let Err(error) = self.stack.apply(&operation) {
                return Err(self.disable(error));
            }
        }
        if body.len() > byte_position {
            let end = character_position + body[byte_position..].chars().count();
            push_token(
                &mut tokens,
                character_position,
                end,
                class_for(&self.stack, &self.assets.scope_map),
            );
        }
        Ok(tokens)
    }

    fn disable(&mut self, error: impl fmt::Display) -> String {
        self.disabled = true;
        error.to_string()
    }
}

fn class_for(
    stack: &ScopeStack,
    scope_map: &[(Scope, SyntaxTokenClass)],
) -> Option<SyntaxTokenClass> {
    for scope in stack.as_slice().iter().rev() {
        if let Some((_, class)) = scope_map
            .iter()
            .find(|(prefix, _)| prefix.is_prefix_of(*scope))
        {
            return Some(*class);
        }
    }
    None
}

fn push_token(
    tokens: &mut Vec<SyntaxToken>,
    start: usize,
    end: usize,
    class: Option<SyntaxTokenClass>,
) {
    let Some(class) = class else { return };
    if start == end {
        return;
    }
    match tokens.last_mut() {
        Some(last) if last.end() == start && last.class() == class => {
            *last = SyntaxToken::new(last.start(), end, class);
        }
        _ => tokens.push(SyntaxToken::new(start, end, class)),
    }
}

const SCOPE_PREFIXES: [(&str, SyntaxTokenClass); 23] = [
    ("comment", SyntaxTokenClass::Comment),
    ("string", SyntaxTokenClass::String),
    ("constant.numeric", SyntaxTokenClass::Number),
    ("constant", SyntaxTokenClass::Constant),
    ("keyword.operator", SyntaxTokenClass::Operator),
    ("keyword", SyntaxTokenClass::Keyword),
    ("storage", SyntaxTokenClass::Keyword),
    ("entity.name.function", SyntaxTokenClass::Function),
    ("support.function", SyntaxTokenClass::Function),
    ("entity.name.type", SyntaxTokenClass::Type),
    ("entity.name.class", SyntaxTokenClass::Type),
    ("entity.name.struct", SyntaxTokenClass::Type),
    ("entity.name.enum", SyntaxTokenClass::Type),
    ("entity.name.trait", SyntaxTokenClass::Type),
    ("support.type", SyntaxTokenClass::Type),
    ("support.class", SyntaxTokenClass::Type),
    ("entity.name.tag", SyntaxTokenClass::Tag),
    ("markup.heading", SyntaxTokenClass::Tag),
    ("markup.raw", SyntaxTokenClass::String),
    ("markup.bold", SyntaxTokenClass::Keyword),
    ("markup.italic", SyntaxTokenClass::Keyword),
    ("entity.other.attribute-name", SyntaxTokenClass::Variable),
    ("variable.parameter", SyntaxTokenClass::Variable),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn required_syntax(path: &str) -> SyntaxDefinition {
        bundled_syntax_catalog()
            .expect("bundled syntax catalog should load")
            .syntax_for_path(path)
            .expect("fixture syntax should be present")
    }

    fn keyword_token_overlaps(tokens: &[SyntaxToken], start: usize, end: usize) -> bool {
        tokens.iter().any(|token| {
            token.class() == SyntaxTokenClass::Keyword && token.start() < end && start < token.end()
        })
    }

    #[test]
    fn corrupt_syntax_pack_is_an_error() {
        assert!(matches!(
            SyntaxCatalog::from_uncompressed_pack(&[]),
            Err(SyntaxCatalogError::SyntaxPack { .. })
        ));
    }

    #[test]
    fn unknown_or_missing_extensions_resolve_none() {
        let catalog = bundled_syntax_catalog().expect("bundled syntax catalog should load");
        for path in ["file.zzzunknown", "no-extension", ""] {
            assert!(catalog.syntax_for_path(path).is_none());
        }
    }

    #[test]
    fn token_spans_are_character_indexed() {
        let mut side = SideHighlighter::new(&required_syntax("a.rs"));
        let tokens = side
            .tokens("let caf\u{e9} = \"\u{3b1}\";")
            .expect("fixture should tokenize");
        let string = tokens
            .iter()
            .find(|token| token.class() == SyntaxTokenClass::String)
            .expect("string token should be present");
        assert_eq!((string.start(), string.end()), (11, 14));
    }

    #[test]
    fn rust_strings_and_comments_receive_semantic_tokens() {
        let tokens = SideHighlighter::new(&required_syntax("a.rs"))
            .tokens(r#"let s = "hi"; // note"#)
            .expect("fixture should tokenize");

        assert!(tokens.iter().any(|token| {
            token.class() == SyntaxTokenClass::String && token.start() == 8 && token.end() == 12
        }));
        assert!(tokens.iter().any(|token| {
            token.class() == SyntaxTokenClass::Comment && token.start() == 14 && token.end() == 21
        }));
    }

    #[test]
    fn typescript_keywords_numbers_and_comments_receive_tokens() {
        let tokens = SideHighlighter::new(&required_syntax("a.ts"))
            .tokens("const n: number = 1; // c")
            .expect("fixture should tokenize");

        for class in [
            SyntaxTokenClass::Keyword,
            SyntaxTokenClass::Number,
            SyntaxTokenClass::Comment,
        ] {
            assert!(tokens.iter().any(|token| token.class() == class));
        }
    }

    #[test]
    fn rust_async_token_contract() {
        for (source, expected_keyword) in [
            ("async fn run() {}", true),
            ("async move {}", true),
            ("let r#async = 1;", false),
            ("let asynchronous = 1;", false),
            (r#"let value = "async";"#, false),
            ("// async", false),
            ("/* async */", false),
        ] {
            let start = source.find("async").expect("fixture should contain async");
            let end = start + "async".len();
            let tokens = SideHighlighter::new(&required_syntax("a.rs"))
                .tokens(source)
                .expect("fixture should tokenize");
            assert_eq!(
                keyword_token_overlaps(&tokens, start, end),
                expected_keyword,
                "unexpected async classification for {source:?}: {tokens:?}"
            );
        }
    }

    #[test]
    fn block_comment_state_carries_across_lines() {
        let mut side = SideHighlighter::new(&required_syntax("a.rs"));
        side.tokens("/* open").expect("fixture should tokenize");
        let tokens = side
            .tokens("still inside")
            .expect("fixture should tokenize");

        assert_eq!(
            tokens,
            vec![SyntaxToken::new(0, 12, SyntaxTokenClass::Comment)]
        );
    }

    #[test]
    fn every_required_extension_yields_tokens() {
        let representative = [
            ("a.js", "const x = 1; // note"),
            ("a.ts", "const x: number = 1;"),
            ("a.py", "x = 1  # note"),
            ("a.rs", "let x = 1; // note"),
            ("a.md", "# heading"),
            ("a.html", "<div class=\"x\">text</div>"),
            ("a.yml", "key: value"),
            ("a.yaml", "key: value"),
        ];
        for (path, line) in representative {
            let tokens = SideHighlighter::new(&required_syntax(path))
                .tokens(line)
                .expect("fixture should tokenize");
            assert!(!tokens.is_empty(), "no tokens for {path}: {line:?}");
        }
    }
}
