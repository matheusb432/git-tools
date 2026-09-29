mod client_diff;
mod client_diff_cache;
mod commit_pages;
mod diff_history;
pub(crate) mod live_errors;
mod rows;
pub(crate) mod viewer_server;

#[cfg(feature = "component-preview")]
pub(crate) use client_diff::static_diff_workspace;
pub(crate) use client_diff::{
    ClientDiffFile, ClientDiffFileState, ClientDiffFileStoreExt, ClientDiffRows,
    ClientDiffRowsStoreExt, ClientDiffWindow, ClientDiffWorkspace, ClientDiffWorkspaceController,
    ClientDiffWorkspaceStoreExt, use_client_diff_workspace,
};
pub(crate) use client_diff_cache::use_client_diff_cache_provider;
pub(crate) use commit_pages::{use_commit_page_cache_provider, use_viewer_commit_pages};
pub(crate) use diff_history::history_navigation;
use gtl_wire::viewer::ViewerRecipeKind;
pub(crate) use rows::{
    ViewerCodeLine, ViewerCodeSpan, ViewerSplitRow, ViewerSyntaxClass, ViewerUnifiedRow,
    ViewerUnifiedSourceRow,
};
pub(crate) fn recipe_kind_label(
    kind: ViewerRecipeKind,
    language: gtl_models::settings::ViewerLanguage,
) -> String {
    use crate::shared::i18n::t;
    match kind {
        ViewerRecipeKind::Diff => t!(language, "history-kind-diff"),
        ViewerRecipeKind::MergeDiff => t!(language, "history-kind-merge-diff"),
    }
}
