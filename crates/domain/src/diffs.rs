//! The diffs feature's domain model.

mod commit;
mod exclusions;
mod kind;

pub use commit::Commit;
pub use exclusions::{AppliedExclusions, DiffExclusions, ExcludedExtensions};
pub use kind::DiffKind;
