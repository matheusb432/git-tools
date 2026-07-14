//! The diffs feature's domain model.

mod commit;
mod exclusions;
mod file;
mod intraline;
mod kind;
mod range;
mod rows;
mod split;
mod target;
mod view;

pub use commit::Commit;
pub use exclusions::{AppliedExclusions, DiffExclusions, ExcludedExtensions};
pub use file::{FileDiff, FileStatus, LineOwners};
pub use intraline::{LineSpans, Span, changed_spans};
pub use kind::DiffKind;
pub use range::{Mode, Ranges, ranges, ranges_over};
pub use rows::{MAX_LINE_COLS, Row, RowKind, derive_rows, is_meta_line, line_body, long_line_len};
pub use split::{SplitCell, SplitRow, split_rows};
pub use target::{DiffTarget, PinnedRange};
pub use view::{Cmd, Foot, View, sort_files_tree_order};
