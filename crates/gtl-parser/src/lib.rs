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
mod split;
#[cfg(feature = "syntax")]
mod syntax;
mod token;

pub use classify::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};
pub use intraline::CharacterSpan;
pub use model::{
    DEFAULT_MAX_LINE_CHARACTERS, DiffParser, DiffRow, DiffRowKind, DiffSide, ParseOptions,
    ParsedDiff, SyntaxDiagnostic, diff_line_body,
};
pub use split::{SplitDiffCell, SplitDiffRow};
#[cfg(feature = "bundled-syntaxes")]
pub use syntax::bundled_syntax_catalog;
#[cfg(feature = "syntax")]
pub use syntax::{SyntaxCatalog, SyntaxCatalogError, SyntaxDefinition};
pub use token::{SyntaxToken, SyntaxTokenClass};
