mod client_diff;
#[cfg(feature = "desktop")]
mod commit_pages;
#[cfg(feature = "desktop")]
mod diff_history;
mod rows;
#[cfg(feature = "desktop")]
pub(crate) mod viewer_server;

#[cfg(test)]
pub(crate) use client_diff::ClientDiffRows;
#[cfg(feature = "artifact")]
pub(crate) use client_diff::static_diff_workspace;
#[cfg(feature = "desktop")]
pub(crate) use client_diff::use_client_diff_workspace;
pub(crate) use client_diff::{
    ClientDiffFile, ClientDiffFileState, ClientDiffFileStoreExt, ClientDiffRowsStoreExt,
    ClientDiffWorkspace, ClientDiffWorkspaceStoreExt,
};
#[cfg(feature = "desktop")]
pub(crate) use commit_pages::use_viewer_commit_pages;
#[cfg(feature = "desktop")]
pub(crate) use diff_history::history_navigation;
#[cfg(feature = "desktop")]
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout, ViewerRecipeKind};
#[cfg(feature = "desktop")]
pub(crate) use rows::ViewerRowEvent;
pub(crate) use rows::{ViewerCodeLine, ViewerSplitRow, ViewerSyntaxClass, ViewerUnifiedRow};
#[cfg(feature = "desktop")]
pub(crate) const fn layout_label(layout: ViewerDiffLayout) -> &'static str {
    match layout {
        ViewerDiffLayout::Unified => "Unified",
        ViewerDiffLayout::Split => "Split",
    }
}

#[cfg(feature = "desktop")]
pub(crate) const fn density_label(density: ViewerDiffDensity) -> &'static str {
    match density {
        ViewerDiffDensity::Compact => "Changes",
        ViewerDiffDensity::Full => "Full file",
    }
}

#[cfg(feature = "desktop")]
pub(crate) const fn recipe_kind_label(kind: ViewerRecipeKind) -> &'static str {
    match kind {
        ViewerRecipeKind::Diff => "Diff",
        ViewerRecipeKind::MergeDiff => "Merge diff",
    }
}
