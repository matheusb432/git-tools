//! The diffs feature: engine utilities, the artifact-producing render slices
//! (`render_diff`/`render_diff_all`/`render_merge_diff`/`render_squash_preview`/
//! `render_diff_subrepos`), and the view-only compute slices the native viewer
//! dispatches (`compute_diff`/`compute_merge_diff`/`compute_squash_preview`).

mod artifacts;
pub mod attribution;
pub mod batch;
pub mod compute_diff;
pub mod compute_merge_diff;
pub mod compute_squash_preview;
mod file;
pub mod open_diff_file_in_configured_editor;
mod range;
mod range_view;
pub mod render_diff;
pub mod render_diff_all;
pub mod render_diff_subrepos;
pub mod render_merge_diff;
pub mod render_squash_preview;
mod target;
mod unified_diff;
pub mod util;
mod view;

pub use batch::RepoRef;
pub use file::{FileDiff, FileStatus, LineOwners};
pub use target::{DiffTarget, DiffTargetRequest, DiffTargetRequestError, PinnedRange};
pub use unified_diff::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};
pub use view::{Cmd, Foot, View, sort_files_tree_order};
