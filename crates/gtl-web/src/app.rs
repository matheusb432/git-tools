use dioxus::prelude::*;

pub(crate) mod application_layout;
pub(crate) mod application_navigation;
pub(crate) mod application_router;

use application_router::Route;

const FAVICON: Asset = asset!("/src/app/assets/app-icon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const DIFF_ISLAND_JS: Asset = asset!("/assets/generated/diff-island.js");
pub(crate) const DIFF_ISLAND_CSS: Asset = asset!("/assets/diff-island.css");

#[component]
pub(crate) fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Script { src: DIFF_ISLAND_JS }
        Router::<Route> {}
    }
}

#[cfg(test)]
mod tests {
    const TAILWIND_SOURCE: &str = include_str!("app/assets/styles/tailwind.css");
    const THEME_NAMES: [&str; 7] = [
        "dark", "light", "hearth", "mirage", "glacier", "noir", "graphite",
    ];
    const SURFACE_TOKENS: [&str; 4] = [
        "--color-bg",
        "--color-surface",
        "--color-surface-2",
        "--color-sunk",
    ];

    #[test]
    fn tertiary_text_meets_normal_text_contrast_on_every_theme_surface() {
        for theme_name in THEME_NAMES {
            let block = theme_block(TAILWIND_SOURCE, theme_name);
            assert!(
                block.is_some(),
                "missing or unterminated {theme_name} theme"
            );
            let Some(block) = block else {
                continue;
            };
            let tertiary_ink = hex_token(block, "--color-ink-3");
            assert!(tertiary_ink.is_some(), "missing {theme_name} tertiary ink");
            let Some(tertiary_ink) = tertiary_ink else {
                continue;
            };

            for surface_token in SURFACE_TOKENS {
                let surface = hex_token(block, surface_token);
                assert!(surface.is_some(), "missing {theme_name} {surface_token}");
                let Some(surface) = surface else {
                    continue;
                };
                let ratio = contrast_ratio(tertiary_ink, surface);
                assert!(
                    ratio >= 4.5,
                    "{theme_name} {surface_token} contrast is {ratio:.3}:1"
                );
            }
        }
    }

    fn theme_block<'source>(source: &'source str, theme_name: &str) -> Option<&'source str> {
        let selector = format!("[data-theme=\"{theme_name}\"] {{");
        source
            .split_once(&selector)
            .and_then(|(_, block)| block.split_once("\n  }"))
            .map(|(block, _)| block)
    }

    fn hex_token(block: &str, token: &str) -> Option<[u8; 3]> {
        let declaration = format!("{token}: #");
        let value = block.split_once(&declaration)?.1;
        let hex = value.get(..6)?;

        Some([
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ])
    }

    fn contrast_ratio(left: [u8; 3], right: [u8; 3]) -> f64 {
        let left = relative_luminance(left);
        let right = relative_luminance(right);
        (left.max(right) + 0.05) / (left.min(right) + 0.05)
    }

    fn relative_luminance(color: [u8; 3]) -> f64 {
        let [red, green, blue] = color.map(linear_channel);
        0.2126 * red + 0.7152 * green + 0.0722 * blue
    }

    fn linear_channel(channel: u8) -> f64 {
        let channel = f64::from(channel) / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }
}
