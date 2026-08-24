mod client_diff;
#[cfg(feature = "desktop")]
mod commit_pages;
#[cfg(feature = "desktop")]
mod diff_history;
mod rows;
#[cfg(feature = "desktop")]
pub(crate) mod viewer_server;

#[cfg(feature = "desktop")]
pub(crate) use client_diff::use_client_diff_workspace;
pub(crate) use client_diff::{ClientDiffFile, ClientDiffFileState, ClientDiffRows};
#[cfg(feature = "artifact")]
pub(crate) use client_diff::{ClientDiffWorkspace, static_diff_workspace};
#[cfg(feature = "desktop")]
pub(crate) use commit_pages::use_viewer_commit_pages;
#[cfg(feature = "desktop")]
pub(crate) use diff_history::history_navigation;
#[cfg(feature = "desktop")]
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout, ViewerRecipeKind, ViewerTheme};
#[cfg(feature = "desktop")]
pub(crate) use rows::ViewerRowEvent;
pub(crate) use rows::{
    ViewerCodeLine, ViewerCodeSpan, ViewerSplitCell, ViewerSplitRow, ViewerSyntaxClass,
    ViewerUnifiedRow, ViewerUnifiedSourceRow,
};
#[cfg(feature = "desktop")]
pub(crate) const fn theme_value(theme: ViewerTheme) -> &'static str {
    theme.as_str()
}

#[cfg(feature = "desktop")]
pub(crate) const fn theme_label(theme: ViewerTheme) -> &'static str {
    match theme {
        ViewerTheme::Dark => "Dark",
        ViewerTheme::Light => "Light",
        ViewerTheme::Hearth => "Hearth",
        ViewerTheme::Mirage => "Mirage",
        ViewerTheme::Glacier => "Glacier",
        ViewerTheme::Noir => "Noir",
        ViewerTheme::Graphite => "Graphite",
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn theme_from_value(value: &str) -> Option<ViewerTheme> {
    match value {
        "dark" => Some(ViewerTheme::Dark),
        "light" => Some(ViewerTheme::Light),
        "hearth" => Some(ViewerTheme::Hearth),
        "mirage" => Some(ViewerTheme::Mirage),
        "glacier" => Some(ViewerTheme::Glacier),
        "noir" => Some(ViewerTheme::Noir),
        "graphite" => Some(ViewerTheme::Graphite),
        _ => None,
    }
}

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
