//! Pure parsing primitives for Git unified diffs.
//!
//! The crate classifies patch lines, derives numbered rows, pairs split rows,
//! computes intraline changes, and can optionally attach semantic syntax
//! tokens. It performs no file system, process, network, or rendering work.

mod classify;
#[cfg(feature = "syntax")]
mod highlight;
mod intraline;
mod model;
mod semantic;
mod split;
#[cfg(feature = "syntax")]
mod syntax;
mod token;

pub use classify::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};
pub use intraline::CharacterSpan;
pub use model::{
    DEFAULT_MAX_LINE_CHARACTERS, DiffParser, DiffParserStream, DiffRow, DiffRowKind, DiffSide,
    ParseOptions, ParsedDiff, ParsedDiffBatch, SyntaxDiagnostic, diff_line_body,
};
pub use semantic::{SemanticTextChange, SemanticTextSpan};
pub use split::{SplitDiffCell, SplitDiffRow, SplitDiffStream};
#[cfg(feature = "bundled-syntaxes")]
pub use syntax::bundled_syntax_catalog;
#[cfg(feature = "syntax")]
pub use syntax::{SyntaxCatalog, SyntaxCatalogError, SyntaxDefinition};
pub use token::{SyntaxToken, SyntaxTokenClass};
