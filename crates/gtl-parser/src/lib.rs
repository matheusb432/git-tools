//! Pure parsing primitives for Git unified diffs.
//!
//! The crate classifies patch lines, derives numbered rows, pairs split rows,
//! computes intraline changes, and can optionally attach semantic syntax
//! tokens. It performs no file system, process, network, or rendering work.

pub mod cancellation;
mod classify;
mod coordinate;
#[cfg(feature = "syntax")]
mod highlight;
pub mod index;
mod intraline;
mod model;
mod semantic;
mod split;
#[cfg(feature = "syntax")]
mod syntax;
mod token;
#[cfg(all(feature = "syntax", any(test, target_arch = "wasm32")))]
mod wasm_allocator;

pub use classify::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};
pub use coordinate::{
    CharacterCount, CharacterOffset, LineNumberDigitWidth, SourceLineNumber, SyntaxHunkByteLimit,
};
pub use intraline::CharacterSpan;
pub use model::{
    DEFAULT_MAX_LINE_CHARACTERS, DEFAULT_MAX_SYNTAX_HUNK_BYTES, DiffParser, DiffParserStream,
    DiffRow, DiffRowKind, DiffSide, ParseOptions, ParsedDiff, ParsedDiffBatch, SyntaxDiagnostic,
    diff_line_body,
};
pub use semantic::{SemanticTextChange, SemanticTextSpan};
pub use split::{SplitDiffCell, SplitDiffRow, SplitDiffStream};
#[cfg(feature = "syntax")]
pub use syntax::SyntaxLanguage;
pub use token::{SyntaxToken, SyntaxTokenClass};
