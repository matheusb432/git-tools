use gtl_wire::viewer::ViewerTheme;

pub(crate) const VIEWER_THEME_OPTIONS: [ViewerTheme; 5] = [
    ViewerTheme::Dark,
    ViewerTheme::Mirage,
    ViewerTheme::Glacier,
    ViewerTheme::Graphite,
    ViewerTheme::Carbon,
];

pub(crate) const fn viewer_theme_label(theme: ViewerTheme) -> &'static str {
    match theme {
        ViewerTheme::Dark => "Dark",
        ViewerTheme::Mirage => "Mirage",
        ViewerTheme::Glacier => "Glacier",
        ViewerTheme::Graphite => "Graphite",
        ViewerTheme::Carbon => "Carbon",
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
