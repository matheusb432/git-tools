//! The diffs feature: engine utilities, the artifact-producing render slices
//! (`render_diff`/`render_diff_all`/`render_merge_diff`/`render_squash_preview`/
//! `render_diff_subrepos`), and the view-only compute slices the native viewer
//! dispatches (`compute_diff`/`compute_merge_diff`/`compute_squash_preview`).

pub mod attribution;
pub mod batch;
pub mod compute_diff;
pub mod compute_merge_diff;
pub mod compute_squash_preview;
mod range;
mod range_view;
pub mod render_diff;
pub mod render_diff_all;
pub mod render_diff_subrepos;
pub mod render_merge_diff;
pub mod render_squash_preview;
pub mod util;

pub use batch::RepoRef;
