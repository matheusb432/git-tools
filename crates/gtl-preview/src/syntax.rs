//! Render-time syntax token computation for diff code lines.
//!
//! Resolves a file's grammar from its extension against the embedded syntax
//! set, then tokenizes one diff side line by line with carried parser state.
//! Output spans are char-indexed over the line body so the row renderer can
//! weave them into its per-char escape walk.

use std::sync::{Arc, LazyLock};

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PreviewError {
    #[error("embedded syntax pack is invalid")]
    SyntaxPack(#[source] anyhow::Error),
    #[error("embedded syntax scope prefix is invalid: {prefix}")]
    ScopePrefix {
        prefix: &'static str,
        #[source]
        source: syntect::parsing::ParseScopeError,
    },
}

/// The result of rendering preview content that depends on embedded syntax assets.
pub type PreviewResult<T> = Result<T, Arc<PreviewError>>;

struct SyntaxAssets {
    syntax_set: SyntaxSet,
    scope_map: Vec<(Scope, TokenClass)>,
}

static SYNTAX_ASSETS: LazyLock<PreviewResult<SyntaxAssets>> = LazyLock::new(|| {
    load_syntax_assets(
        include_bytes!("embedded/generated/syntaxes.packdump"),
        &SCOPE_PREFIXES,
    )
    .map_err(Arc::new)
});

fn load_syntax_assets(
    syntax_pack: &[u8],
    scope_prefixes: &[(&'static str, TokenClass)],
) -> Result<SyntaxAssets, PreviewError> {
    let syntax_set = syntect::dumps::from_uncompressed_data(syntax_pack)
        .map_err(|source| PreviewError::SyntaxPack(anyhow::Error::new(source)))?;
    let scope_map = scope_prefixes
        .iter()
        .copied()
        .map(|(prefix, class)| {
            Scope::new(prefix)
                .map(|scope| (scope, class))
                .map_err(|source| PreviewError::ScopePrefix { prefix, source })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SyntaxAssets {
        syntax_set,
        scope_map,
    })
}

fn syntax_assets() -> PreviewResult<&'static SyntaxAssets> {
    SYNTAX_ASSETS.as_ref().map_err(Arc::clone)
}

#[derive(Clone, Copy)]
pub(crate) struct SyntaxDefinition {
    syntax: &'static SyntaxReference,
    syntax_set: &'static SyntaxSet,
    scope_map: &'static [(Scope, TokenClass)],
}

/// The grammar for `path`, resolved by extension, or `None` when the set has
/// no match (the file renders unhighlighted).
pub(crate) fn syntax_for_path(path: &str) -> PreviewResult<Option<SyntaxDefinition>> {
    let Some(ext) = std::path::Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
    else {
        return Ok(None);
    };
    let assets = syntax_assets()?;
    Ok(assets
        .syntax_set
        .find_syntax_by_extension(ext)
        .map(|syntax| SyntaxDefinition {
            syntax,
            syntax_set: &assets.syntax_set,
            scope_map: &assets.scope_map,
        }))
}

/// Semantic token classes the themes color; one CSS custom property each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenClass {
    Keyword,
    String,
    Comment,
    Type,
    Function,
    Number,
    Constant,
    Operator,
    Tag,
    Variable,
}

impl TokenClass {
    /// The stable short CSS class name for this token class (e.g. `sy-kw`),
    /// combined with `ciw` in emitted markup where a span is both a syntax
    /// token and an intra-line changed run.
    pub(crate) const fn css_class(self) -> &'static str {
        match self {
            TokenClass::Keyword => "sy-kw",
            TokenClass::String => "sy-str",
            TokenClass::Comment => "sy-com",
            TokenClass::Type => "sy-typ",
            TokenClass::Function => "sy-fn",
            TokenClass::Number => "sy-num",
            TokenClass::Constant => "sy-con",
            TokenClass::Operator => "sy-op",
            TokenClass::Tag => "sy-tag",
            TokenClass::Variable => "sy-var",
        }
    }
}

// ! Priority-ordered prefix map from TextMate scopes to token classes; first
// ! match on a stack scope wins, and the stack is walked innermost-first so
// ! the most specific scope decides.
const SCOPE_PREFIXES: [(&str, TokenClass); 23] = [
    ("comment", TokenClass::Comment),
    ("string", TokenClass::String),
    ("constant.numeric", TokenClass::Number),
    ("constant", TokenClass::Constant),
    ("keyword.operator", TokenClass::Operator),
    ("keyword", TokenClass::Keyword),
    ("storage", TokenClass::Keyword),
    ("entity.name.function", TokenClass::Function),
    ("support.function", TokenClass::Function),
    ("entity.name.type", TokenClass::Type),
    ("entity.name.class", TokenClass::Type),
    ("entity.name.struct", TokenClass::Type),
    ("entity.name.enum", TokenClass::Type),
    ("entity.name.trait", TokenClass::Type),
    ("support.type", TokenClass::Type),
    ("support.class", TokenClass::Type),
    ("entity.name.tag", TokenClass::Tag),
    ("markup.heading", TokenClass::Tag),
    ("markup.raw", TokenClass::String),
    ("markup.bold", TokenClass::Keyword),
    ("markup.italic", TokenClass::Keyword),
    ("entity.other.attribute-name", TokenClass::Variable),
    ("variable.parameter", TokenClass::Variable),
];

fn class_for(stack: &ScopeStack, scope_map: &[(Scope, TokenClass)]) -> Option<TokenClass> {
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

/// A half-open char-index range of one token class within a line body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) class: TokenClass,
}

/// Line-by-line tokenizer for one side of a file's diff, carrying parser
/// state across lines. A parser failure disables highlighting for the side so
/// remaining lines render as plain text.
pub(crate) struct SideHighlighter {
    parse: ParseState,
    stack: ScopeStack,
    syntax_set: &'static SyntaxSet,
    scope_map: &'static [(Scope, TokenClass)],
    disabled: bool,
}

impl SideHighlighter {
    pub(crate) fn new(definition: SyntaxDefinition) -> Self {
        Self {
            parse: ParseState::new(definition.syntax),
            stack: ScopeStack::new(),
            syntax_set: definition.syntax_set,
            scope_map: definition.scope_map,
            disabled: false,
        }
    }

    /// Tokens for one line body (diff marker stripped, no trailing newline).
    pub(crate) fn tokens(&mut self, body: &str) -> Vec<Token> {
        if self.disabled {
            return Vec::new();
        }
        // ! The newline-variant grammars require the terminator to match
        // ! line-end rules; ops past body.len() refer to it and are clamped.
        let line = format!("{body}\n");
        let ops = match self.parse.parse_line(&line, self.syntax_set) {
            Ok(ops) => ops,
            Err(error) => return self.disable(error),
        };

        let mut tokens: Vec<Token> = Vec::new();
        let mut byte_pos = 0usize;
        let mut char_pos = 0usize;
        for (op_byte, op) in ops {
            let clamped = op_byte.min(body.len());
            if clamped > byte_pos {
                let end = char_pos + body[byte_pos..clamped].chars().count();
                push_token(
                    &mut tokens,
                    char_pos,
                    end,
                    class_for(&self.stack, self.scope_map),
                );
                char_pos = end;
                byte_pos = clamped;
            }
            if let Err(error) = self.stack.apply(&op) {
                return self.disable(error);
            }
        }
        if body.len() > byte_pos {
            let end = char_pos + body[byte_pos..].chars().count();
            push_token(
                &mut tokens,
                char_pos,
                end,
                class_for(&self.stack, self.scope_map),
            );
        }
        tokens
    }

    fn disable(&mut self, error: impl std::fmt::Display) -> Vec<Token> {
        self.disabled = true;
        eprintln!("gtl-preview: syntax highlighting disabled: {error}");
        Vec::new()
    }
}

/// Append a classed run, extending the previous token when it abuts with the
/// same class so grammar-internal splits collapse into one span.
fn push_token(tokens: &mut Vec<Token>, start: usize, end: usize, class: Option<TokenClass>) {
    let Some(class) = class else { return };
    if start == end {
        return;
    }
    match tokens.last_mut() {
        Some(last) if last.end == start && last.class == class => last.end = end,
        _ => tokens.push(Token { start, end, class }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required_syntax(path: &str) -> SyntaxDefinition {
        syntax_for_path(path)
            .expect("embedded syntax assets should load")
            .expect("fixture syntax should be present")
    }

    fn keyword_token_overlaps(tokens: &[Token], start: usize, end: usize) -> bool {
        tokens.iter().any(|token| {
            token.class == TokenClass::Keyword && token.start < end && start < token.end
        })
    }

    #[test]
    fn corrupt_embedded_syntax_pack_is_a_render_error() {
        let Err(error) = load_syntax_assets(&[], &SCOPE_PREFIXES) else {
            panic!("corrupt syntax pack should fail");
        };

        assert!(matches!(error, PreviewError::SyntaxPack(_)));
    }

    #[test]
    fn unknown_or_missing_extensions_resolve_none() {
        for path in ["file.zzzunknown", "no-extension", ""] {
            assert!(
                syntax_for_path(path)
                    .expect("embedded syntax assets should load")
                    .is_none()
            );
        }
    }

    #[test]
    fn rust_string_and_comment_token_spans_are_char_indexed() {
        let mut side = SideHighlighter::new(required_syntax("a.rs"));
        let tokens = side.tokens(r#"let s = "hi"; // note"#);
        assert!(
            tokens.contains(&Token {
                start: 8,
                end: 12,
                class: TokenClass::String
            }),
            "string span missing: {tokens:?}"
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.class == TokenClass::Comment && t.start == 14 && t.end == 21),
            "comment span missing: {tokens:?}"
        );
    }

    #[test]
    fn token_indices_count_chars_not_bytes() {
        let mut side = SideHighlighter::new(required_syntax("a.rs"));
        // 'e' with accent is 2 bytes, alpha is 2 bytes: byte indexing would shift the span.
        let tokens = side.tokens("let caf\u{e9} = \"\u{3b1}\";");
        let string = tokens
            .iter()
            .find(|t| t.class == TokenClass::String)
            .unwrap();
        assert_eq!((string.start, string.end), (11, 14), "{tokens:?}");
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
            let start = source.find("async").unwrap();
            let end = start + "async".len();
            let mut side = SideHighlighter::new(required_syntax("a.rs"));
            let tokens = side.tokens(source);
            if expected_keyword {
                assert!(
                    tokens.contains(&Token {
                        start,
                        end,
                        class: TokenClass::Keyword,
                    }),
                    "async keyword span missing for {source:?}: {tokens:?}"
                );
            } else {
                assert!(
                    !keyword_token_overlaps(&tokens, start, end),
                    "async keyword span present for {source:?}: {tokens:?}"
                );
            }
        }
    }

    #[test]
    fn block_comment_state_carries_across_lines() {
        let mut side = SideHighlighter::new(required_syntax("a.rs"));
        side.tokens("/* open");
        let tokens = side.tokens("still inside");
        assert_eq!(
            tokens,
            vec![Token {
                start: 0,
                end: 12,
                class: TokenClass::Comment
            }]
        );
    }

    #[test]
    fn typescript_tokenizes_keywords_and_comments() {
        let mut side = SideHighlighter::new(required_syntax("a.ts"));
        let tokens = side.tokens("const n: number = 1; // c");
        assert!(tokens.iter().any(|t| t.class == TokenClass::Keyword));
        assert!(tokens.iter().any(|t| t.class == TokenClass::Number));
        assert!(tokens.iter().any(|t| t.class == TokenClass::Comment));
    }

    #[test]
    fn every_required_extension_yields_tokens_on_representative_code() {
        let representative: &[(&str, &str)] = &[
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
            let syntax = required_syntax(path);
            let tokens = SideHighlighter::new(syntax).tokens(line);
            assert!(!tokens.is_empty(), "no tokens for {path}: {line:?}");
        }
    }
}
