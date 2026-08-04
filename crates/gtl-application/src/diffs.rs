//! The diffs feature: engine utilities, the artifact-producing render slices
//! (`render_diff`/`render_diff_all`/`render_merge_diff`/`render_squash_preview`/
//! `render_diff_subrepos`), and the view-only compute slices the native viewer
//! dispatches (`compute_diff`/`compute_merge_diff`/`compute_squash_preview`).

pub mod compute_commit_patch;
pub mod compute_diff;
pub mod compute_merge_diff;
pub mod compute_squash_preview;
mod diff_computation;
mod logic;
pub mod open_diff_file_in_configured_editor;
pub mod present_diff;
pub mod render_diff;
pub mod render_diff_all;
pub mod render_diff_subrepos;
pub mod render_merge_diff;
pub mod render_squash_preview;

pub use logic::{
    batch::RepoRef,
    file::{FileDiff, FileStatus},
    target::{DiffTarget, DiffTargetRequest, DiffTargetRequestError, PinnedRange},
    unified_diff::{UnifiedDiffLineClassifier, UnifiedDiffLineKind},
    view::{Cmd, Foot, View},
};
