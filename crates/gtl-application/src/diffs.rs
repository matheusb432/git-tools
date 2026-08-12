//! The diffs feature: engine utilities, the artifact-producing render slices
//! (`render_diff`/`render_diff_all`/`render_merge_diff`/`render_diff_subrepos`),
//! and the view-only compute slices the native viewer dispatches
//! (`compute_diff`/`compute_merge_diff`).

mod artifacts;
mod assemble;
mod batch;
pub mod compute_commit_patch;
pub mod compute_diff;
pub mod compute_merge_diff;
mod diff_computation;
mod exclusions;
mod file;
pub mod open_diff_file_in_configured_editor;
pub mod present_diff;
mod range;
mod range_view;
pub mod render_diff;
pub mod render_diff_all;
pub mod render_diff_subrepos;
pub mod render_merge_diff;
mod target;
mod view;

pub use batch::RepoRef;
pub use file::{FileDiff, FileStatus};
pub use target::{DiffTarget, DiffTargetRequest, DiffTargetRequestError, PinnedRange};
pub use view::{Cmd, Foot, View};
