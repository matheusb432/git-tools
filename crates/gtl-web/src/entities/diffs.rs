mod client_diff;
#[cfg(feature = "desktop")]
mod client_diff_cache;
#[cfg(feature = "desktop")]
mod commit_pages;
#[cfg(feature = "desktop")]
mod diff_history;
#[cfg(feature = "desktop")]
pub(crate) mod live_errors;
mod rows;
#[cfg(feature = "desktop")]
pub(crate) mod viewer_server;

#[cfg(any(test, feature = "desktop"))]
pub(crate) use client_diff::ClientDiffRows;
#[cfg(feature = "artifact")]
pub(crate) use client_diff::static_diff_workspace;
pub(crate) use client_diff::{
    ClientDiffFile, ClientDiffFileState, ClientDiffFileStoreExt, ClientDiffRowsStoreExt,
    ClientDiffWorkspace, ClientDiffWorkspaceStoreExt,
};
#[cfg(feature = "desktop")]
pub(crate) use client_diff::{
    ClientDiffWindow, ClientDiffWorkspaceController, use_client_diff_workspace,
};
#[cfg(feature = "desktop")]
pub(crate) use client_diff_cache::use_client_diff_cache_provider;
#[cfg(feature = "desktop")]
pub(crate) use commit_pages::{use_commit_page_cache_provider, use_viewer_commit_pages};
#[cfg(feature = "desktop")]
pub(crate) use diff_history::history_navigation;
#[cfg(feature = "desktop")]
use gtl_wire::viewer::ViewerRecipeKind;
pub(crate) use rows::{ViewerCodeLine, ViewerSplitRow, ViewerSyntaxClass, ViewerUnifiedRow};
#[cfg(feature = "desktop")]
pub(crate) const fn recipe_kind_label(kind: ViewerRecipeKind) -> &'static str {
    match kind {
        ViewerRecipeKind::Diff => "Diff",
        ViewerRecipeKind::MergeDiff => "Merge diff",
    }
}
