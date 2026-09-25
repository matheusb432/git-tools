//! The diffs feature's models model.

mod commit;
mod counts;
mod exclusions;
mod kind;
mod view_title;

pub use commit::{Commit, CommitId, CommitIdAbbreviation, CommitIdError, PinnedRange};
pub use counts::DiffLineCount;
pub use exclusions::{AppliedExclusions, DiffExclusions, ExcludedExtensions};
pub use kind::DiffKind;
pub use view_title::DiffViewTitle;
