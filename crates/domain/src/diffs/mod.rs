//! The diffs feature's domain model.

mod commit;
mod file;
mod kind;
mod range;
mod target;
mod view;

pub use commit::Commit;
pub use file::{FileDiff, FileStatus, LineOwners};
pub use kind::DiffKind;
pub use range::{Mode, Ranges, ranges};
pub use target::DiffTarget;
pub use view::{Cmd, Foot, View, sort_files_tree_order};
