//! The diffs feature: engine utilities and the `render_diff/render_diff_all`/
//! `render_merge_diff/render_squash_preview/render_diff_subrepos` slices.

pub mod attribution;
pub mod batch;
pub mod render_diff;
pub mod render_diff_all;
pub mod render_diff_subrepos;
pub mod render_merge_diff;
pub mod render_squash_preview;
pub mod util;

pub use batch::RepoRef;
