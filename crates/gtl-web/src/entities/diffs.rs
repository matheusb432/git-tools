mod api;
mod client_diff;
mod diff_history;

pub(crate) use api::DiffViewerApi;
pub(crate) use client_diff::{
    ClientDiffFile, ClientDiffFileState, ClientDiffRows, ClientDiffSource,
    use_client_diff_workspace,
};
pub(crate) use diff_history::{DiffHistoryApi, history_navigation};
use gtl_contracts::viewer::{ViewerDiffDensity, ViewerDiffLayout, ViewerRecipeKind, ViewerTheme};

pub(crate) const fn theme_value(theme: ViewerTheme) -> &'static str {
    match theme {
        ViewerTheme::Dark => "dark",
        ViewerTheme::Light => "light",
        ViewerTheme::Hearth => "hearth",
        ViewerTheme::Mirage => "mirage",
        ViewerTheme::Glacier => "glacier",
        ViewerTheme::Noir => "noir",
        ViewerTheme::Graphite => "graphite",
    }
}

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

pub(crate) const fn layout_label(layout: ViewerDiffLayout) -> &'static str {
    match layout {
        ViewerDiffLayout::Unified => "Unified",
        ViewerDiffLayout::Split => "Split",
    }
}

pub(crate) const fn density_label(density: ViewerDiffDensity) -> &'static str {
    match density {
        ViewerDiffDensity::Compact => "Changes",
        ViewerDiffDensity::Full => "Full file",
    }
}

pub(crate) const fn recipe_kind_label(kind: ViewerRecipeKind) -> &'static str {
    match kind {
        ViewerRecipeKind::Diff => "Diff",
        ViewerRecipeKind::MergeDiff => "Merge diff",
    }
}
