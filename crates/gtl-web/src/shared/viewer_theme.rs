use gtl_wire::viewer::ViewerTheme;

pub(crate) const VIEWER_THEME_OPTIONS: [ViewerTheme; 7] = [
    ViewerTheme::Dark,
    ViewerTheme::Light,
    ViewerTheme::Hearth,
    ViewerTheme::Mirage,
    ViewerTheme::Glacier,
    ViewerTheme::Noir,
    ViewerTheme::Graphite,
];

pub(crate) const fn viewer_theme_label(theme: ViewerTheme) -> &'static str {
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

pub(crate) fn viewer_theme_from_value(value: &str) -> Option<ViewerTheme> {
    VIEWER_THEME_OPTIONS
        .into_iter()
        .find(|theme| theme.as_str() == value)
}

#[cfg(test)]
mod tests {
    use super::{VIEWER_THEME_OPTIONS, viewer_theme_from_value};

    #[test]
    fn supported_theme_values_round_trip() {
        for theme in VIEWER_THEME_OPTIONS {
            assert_eq!(viewer_theme_from_value(theme.as_str()), Some(theme));
        }
        assert_eq!(viewer_theme_from_value("unknown"), None);
    }
}
