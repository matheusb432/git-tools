//! The diffs feature: engine utilities, the artifact-producing render slices
//! (`render_diff`/`render_project_diff`/`render_merge_diff`/`render_diff_subrepos`),
//! and the view-only compute slices the native viewer dispatches
//! (`compute_diff`/`compute_merge_diff`).

pub(crate) mod artifacts;
mod assemble;
pub(crate) mod batch;
mod changes_since;
pub mod compute_commit_patch;
pub mod compute_diff;
pub mod compute_merge_diff;
mod diff_computation;
mod extension_filter_note;
pub mod fetch_full_context_diff;
mod file;
pub mod file_filter;
pub mod get_diff_file_reviews;
pub mod get_repository_extension_filter;
pub mod open_diff_file_in_configured_editor;
pub mod present_diff;
mod range;
mod range_view;
pub mod read_terminal_diff;
pub mod render_diff;
pub mod render_diff_subrepos;
pub mod render_merge_diff;
pub(crate) mod review;
pub mod save_repository_extension_filter;
pub mod set_diff_extension_filter;
pub mod set_diff_file_reviewed;
pub mod source_lines;
mod target;
mod unified_diff;
mod view;

pub use batch::RepoRef;
pub use fetch_full_context_diff::FetchFullContextDiff;
pub use file::{FileDiff, FileStatus};
pub use target::{DiffTarget, DiffTargetRequest, DiffTargetRequestError, PinnedRange};
pub use view::{
    Cmd, Foot, FullContextDiff, FullContextDiffSource, FullContextDiffState,
    FullContextDiffTransitionError, View,
};

const EMPTY_TREE_ID: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const EMPTY_TREE_ABBREVIATED_ID: &str = "4b825dc642";
