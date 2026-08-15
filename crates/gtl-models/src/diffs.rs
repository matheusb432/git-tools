//! The diffs feature's models model.

mod commit;
mod exclusions;
mod kind;

pub use commit::{Commit, CommitId, CommitIdAbbreviation, CommitIdError, PinnedRange};
pub use exclusions::{AppliedExclusions, DiffExclusions, ExcludedExtensions};
pub use kind::DiffKind;
