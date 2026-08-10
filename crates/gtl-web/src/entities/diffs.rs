mod api;
mod diff_history;
mod diff_island;

pub(crate) use api::DiffViewerApi;
pub(crate) use diff_history::{DiffHistoryApi, history_navigation};
pub(crate) use diff_island::{DiffIslandAppendResult, DiffIslandBridge};
use gtl_contracts::viewer::{
    ViewerDiffDensity, ViewerDiffLayout, ViewerRecipeKind, ViewerTheme, ViewerViewIdentity,
};

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

pub(crate) fn view_identity_value(identity: ViewerViewIdentity) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        identity.tab_id,
        identity.range_generation,
        identity.selection_generation,
        theme_safe_layout_value(identity.render_options.layout),
        theme_safe_density_value(identity.render_options.density)
    )
}

const fn theme_safe_layout_value(layout: ViewerDiffLayout) -> &'static str {
    match layout {
        ViewerDiffLayout::Unified => "unified",
        ViewerDiffLayout::Split => "split",
    }
}

const fn theme_safe_density_value(density: ViewerDiffDensity) -> &'static str {
    match density {
        ViewerDiffDensity::Compact => "compact",
        ViewerDiffDensity::Full => "full",
    }
}

#[cfg(test)]
mod tests {
    use gtl_contracts::viewer::{
        ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions, ViewerViewIdentity,
    };

    use super::view_identity_value;

    #[test]
    fn island_identity_changes_for_every_rendering_generation_and_option() {
        let identity = ViewerViewIdentity {
            tab_id: 7,
            range_generation: 3,
            selection_generation: 2,
            render_options: ViewerRenderOptions {
                layout: ViewerDiffLayout::Split,
                density: ViewerDiffDensity::Full,
            },
        };

        assert_eq!(view_identity_value(identity), "7:3:2:split:full");
    }
}
