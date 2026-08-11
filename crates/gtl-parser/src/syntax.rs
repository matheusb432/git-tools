#[cfg(feature = "bundled-syntaxes")]
use std::{
    collections::{BTreeSet, VecDeque},
    sync::LazyLock,
};
use std::{fmt, path::Path, sync::Arc};

#[cfg(feature = "bundled-syntaxes")]
use serde::{Deserialize, Serialize};
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};
#[cfg(feature = "bundled-syntaxes")]
use syntect::parsing::{
    SyntaxSetBuilder,
    syntax_definition::{self, ContextId, ContextReference, MatchOperation, Pattern},
};

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

/// A deterministic syntax pack selected from the bundled grammar catalog.
#[cfg(feature = "bundled-syntaxes")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedSyntaxPack {
    bytes: Vec<u8>,
    grammar_names: Vec<String>,
}

#[cfg(feature = "bundled-syntaxes")]
impl SelectedSyntaxPack {
    /// Returns the uncompressed bytes accepted by [`SyntaxCatalog::from_uncompressed_pack`].
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the selected grammar names in their stable pack order.
    pub fn grammar_names(&self) -> &[String] {
        &self.grammar_names
    }
}

/// A failure while selecting a minimal pack from the bundled grammar catalog.
#[cfg(feature = "bundled-syntaxes")]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SelectSyntaxPackError {
    #[error("bundled syntax catalog is invalid: {message}")]
    BundledCatalog { message: String },
    #[error("bundled syntax dependency is invalid: {message}")]
    Dependency { message: String },
    #[error("selected syntax pack could not be serialized: {message}")]
    Serialize { message: String },
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

/// Selects the bundled grammars required by the supplied file paths.
#[cfg(feature = "bundled-syntaxes")]
pub fn select_bundled_syntax_pack<'path>(
    paths: impl IntoIterator<Item = &'path str>,
) -> Result<SelectedSyntaxPack, SelectSyntaxPackError> {
    let catalog =
        bundled_syntax_catalog().map_err(|error| SelectSyntaxPackError::BundledCatalog {
            message: error.to_string(),
        })?;
    select_syntax_pack(&catalog.assets.syntax_set, paths)
}

#[cfg(feature = "bundled-syntaxes")]
fn select_syntax_pack<'path>(
    syntax_set: &SyntaxSet,
    paths: impl IntoIterator<Item = &'path str>,
) -> Result<SelectedSyntaxPack, SelectSyntaxPackError> {
    let definitions = syntax_set.clone().into_builder().syntaxes().to_vec();
    let mut selected = BTreeSet::new();
    let plain_text_index = syntax_index_by_name(syntax_set, "Plain Text").ok_or_else(|| {
        SelectSyntaxPackError::Dependency {
            message: "plain-text fallback is missing".to_owned(),
        }
    })?;
    selected.insert(plain_text_index);

    for path in paths {
        let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if let Some(syntax) = syntax_set.find_syntax_by_extension(extension)
            && let Some(index) = syntax_index(syntax_set, syntax)
        {
            selected.insert(index);
        }
    }

    let mut pending = selected.iter().copied().collect::<VecDeque<_>>();
    while let Some(syntax_index) = pending.pop_front() {
        let definition =
            definitions
                .get(syntax_index)
                .ok_or_else(|| SelectSyntaxPackError::Dependency {
                    message: format!("syntax index {syntax_index} is out of bounds"),
                })?;
        for dependency in syntax_dependencies(definition)? {
            validate_context_target(&definitions, dependency)?;
            if selected.insert(dependency.0) {
                pending.push_back(dependency.0);
            }
        }
    }

    let mut builder = SyntaxSetBuilder::new();
    for syntax_index in selected {
        let mut definition = definitions[syntax_index].clone();
        rewrite_definition_references(&definitions, syntax_index, &mut definition)?;
        builder.add(definition);
    }
    let selected_set = builder.build();
    let unresolved = selected_set.find_unlinked_contexts();
    if !unresolved.is_empty() {
        return Err(SelectSyntaxPackError::Dependency {
            message: unresolved.into_iter().collect::<Vec<_>>().join("; "),
        });
    }
    let grammar_names = selected_set
        .syntaxes()
        .iter()
        .map(|syntax| syntax.name.clone())
        .collect();
    let bytes =
        bincode::serialize(&selected_set).map_err(|error| SelectSyntaxPackError::Serialize {
            message: error.to_string(),
        })?;
    Ok(SelectedSyntaxPack {
        bytes,
        grammar_names,
    })
}

#[cfg(feature = "bundled-syntaxes")]
fn syntax_index_by_name(syntax_set: &SyntaxSet, name: &str) -> Option<usize> {
    syntax_set
        .find_syntax_by_name(name)
        .and_then(|syntax| syntax_index(syntax_set, syntax))
}

#[cfg(feature = "bundled-syntaxes")]
fn syntax_index(
    syntax_set: &SyntaxSet,
    syntax: &syntect::parsing::SyntaxReference,
) -> Option<usize> {
    syntax_set
        .syntaxes()
        .iter()
        .position(|candidate| std::ptr::eq(candidate, syntax))
}

#[cfg(feature = "bundled-syntaxes")]
fn syntax_dependencies(
    definition: &syntax_definition::SyntaxDefinition,
) -> Result<BTreeSet<(usize, usize)>, SelectSyntaxPackError> {
    let mut dependencies = BTreeSet::new();
    for context in definition.contexts.values() {
        if let Some(prototype) = context.prototype {
            dependencies.insert(context_coordinates(prototype)?);
        }
        for pattern in &context.patterns {
            match pattern {
                Pattern::Include(reference) => {
                    collect_direct_reference(reference, &mut dependencies)?;
                }
                Pattern::Match(pattern) => {
                    if let Some(reference) = &pattern.with_prototype {
                        collect_direct_reference(reference, &mut dependencies)?;
                    }
                    match &pattern.operation {
                        MatchOperation::Push(references) | MatchOperation::Set(references) => {
                            for reference in references {
                                collect_direct_reference(reference, &mut dependencies)?;
                            }
                        }
                        MatchOperation::Pop | MatchOperation::None => {}
                    }
                }
            }
        }
    }
    Ok(dependencies)
}

#[cfg(feature = "bundled-syntaxes")]
fn collect_direct_reference(
    reference: &ContextReference,
    dependencies: &mut BTreeSet<(usize, usize)>,
) -> Result<(), SelectSyntaxPackError> {
    if let Ok(context) = reference.id() {
        dependencies.insert(context_coordinates(context)?);
    }
    Ok(())
}

#[cfg(feature = "bundled-syntaxes")]
fn context_coordinates(context: ContextId) -> Result<(usize, usize), SelectSyntaxPackError> {
    let bytes =
        bincode::serialize(&context).map_err(|error| SelectSyntaxPackError::Dependency {
            message: format!("serialize context address: {error}"),
        })?;
    bincode::deserialize(&bytes).map_err(|error| SelectSyntaxPackError::Dependency {
        message: format!("decode context address: {error}"),
    })
}

#[cfg(feature = "bundled-syntaxes")]
fn validate_context_target(
    definitions: &[syntax_definition::SyntaxDefinition],
    coordinates: (usize, usize),
) -> Result<(), SelectSyntaxPackError> {
    let (syntax_index, context_index) = coordinates;
    let definition =
        definitions
            .get(syntax_index)
            .ok_or_else(|| SelectSyntaxPackError::Dependency {
                message: format!("context target syntax {syntax_index} is out of bounds"),
            })?;
    context_name(definition, context_index)?;
    Ok(())
}

#[cfg(feature = "bundled-syntaxes")]
#[derive(Deserialize, Serialize)]
enum ContextReferenceWire {
    Named(String),
    ByScope {
        scope: Scope,
        sub_context: Option<String>,
        with_escape: bool,
    },
    File {
        name: String,
        sub_context: Option<String>,
        with_escape: bool,
    },
    Inline(String),
    Direct(ContextId),
}

#[cfg(feature = "bundled-syntaxes")]
fn context_name(
    definition: &syntax_definition::SyntaxDefinition,
    context_index: usize,
) -> Result<&str, SelectSyntaxPackError> {
    let mut names = definition
        .contexts
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
        .get(context_index)
        .copied()
        .ok_or_else(|| SelectSyntaxPackError::Dependency {
            message: format!(
                "context index {context_index} is out of bounds for {}",
                definition.name
            ),
        })
}

#[cfg(feature = "bundled-syntaxes")]
fn rewrite_definition_references(
    definitions: &[syntax_definition::SyntaxDefinition],
    syntax_index: usize,
    definition: &mut syntax_definition::SyntaxDefinition,
) -> Result<(), SelectSyntaxPackError> {
    for context in definition.contexts.values_mut() {
        context.prototype = None;
        for pattern in &mut context.patterns {
            match pattern {
                Pattern::Include(reference) => {
                    rewrite_reference(definitions, syntax_index, reference)?;
                }
                Pattern::Match(pattern) => {
                    if let Some(reference) = &mut pattern.with_prototype {
                        rewrite_reference(definitions, syntax_index, reference)?;
                    }
                    match &mut pattern.operation {
                        MatchOperation::Push(references) | MatchOperation::Set(references) => {
                            for reference in references {
                                rewrite_reference(definitions, syntax_index, reference)?;
                            }
                        }
                        MatchOperation::Pop | MatchOperation::None => {}
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(feature = "bundled-syntaxes")]
fn rewrite_reference(
    definitions: &[syntax_definition::SyntaxDefinition],
    source_syntax_index: usize,
    reference: &mut ContextReference,
) -> Result<(), SelectSyntaxPackError> {
    let Ok(context) = reference.id() else {
        return Ok(());
    };
    let coordinates = context_coordinates(context)?;
    validate_context_target(definitions, coordinates)?;
    let target_definition = &definitions[coordinates.0];
    let target_context = context_name(target_definition, coordinates.1)?.to_owned();
    let wire = if coordinates.0 == source_syntax_index {
        ContextReferenceWire::Named(target_context)
    } else {
        ContextReferenceWire::ByScope {
            scope: target_definition.scope,
            sub_context: Some(target_context),
            with_escape: false,
        }
    };
    let bytes = bincode::serialize(&wire).map_err(|error| SelectSyntaxPackError::Dependency {
        message: format!("serialize rewritten context reference: {error}"),
    })?;
    *reference =
        bincode::deserialize(&bytes).map_err(|error| SelectSyntaxPackError::Dependency {
            message: format!("decode rewritten context reference: {error}"),
        })?;
    Ok(())
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
    fn rust_selection_contains_only_rust_and_plain_text() {
        let selected = select_bundled_syntax_pack(["src/lib.rs"])
            .expect("Rust syntax selection should succeed");

        assert_eq!(
            selected
                .grammar_names()
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["Plain Text", "Rust"])
        );
        assert!(
            !selected
                .grammar_names()
                .iter()
                .any(|name| { matches!(name.as_str(), "JavaScript" | "Python" | "C++") })
        );
        assert!(selected.as_bytes().len() < 128 * 1024);
    }

    #[test]
    fn multi_language_selection_is_order_independent_and_preserves_tokens() {
        let first =
            select_bundled_syntax_pack(["src/lib.rs", "web/index.html", "web/application.js"])
                .expect("multi-language syntax selection should succeed");
        let second = select_bundled_syntax_pack([
            "web/application.js",
            "src/lib.rs",
            "web/index.html",
            "src/lib.rs",
        ])
        .expect("reordered syntax selection should succeed");
        assert_eq!(first, second);

        let minimal = SyntaxCatalog::from_uncompressed_pack(first.as_bytes())
            .expect("selected syntax pack should load");
        let bundled = bundled_syntax_catalog().expect("bundled syntax catalog should load");
        for (path, line) in [
            ("src/lib.rs", r#"let value = \"rust\";"#),
            ("web/index.html", r#"<main class=\"app\">hello</main>"#),
            ("web/application.js", r#"const value = \"javascript\";"#),
        ] {
            let minimal_tokens = SideHighlighter::new(
                &minimal
                    .syntax_for_path(path)
                    .expect("minimal grammar should exist"),
            )
            .tokens(line)
            .expect("minimal grammar should tokenize");
            let bundled_tokens = SideHighlighter::new(
                &bundled
                    .syntax_for_path(path)
                    .expect("bundled grammar should exist"),
            )
            .tokens(line)
            .expect("bundled grammar should tokenize");
            assert_eq!(minimal_tokens, bundled_tokens, "token parity for {path}");
        }
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
