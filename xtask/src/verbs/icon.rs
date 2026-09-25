//! Generates launcher, favicon, and tray assets from the compact Repository Hub SVG.

use std::{fs, path::Path};

use anyhow::{Context, Result, ensure};
use resvg::{tiny_skia, usvg};

const MARK: &str = include_str!("../../../crates/gtl-web/src/app/assets/repository-hub.svg");
const THEME_TOKENS: &str = include_str!("../../../crates/gtl-web/src/app/assets/styles/tokens.css");
/// Colors the launcher, favicon, and bundle icons, which cannot follow the selected theme.
const DEFAULT_THEME: &str = "dark";
const ICO_SIZES: &[u16] = &[16, 24, 32, 48, 64, 128, 256];
const THEME_LAUNCHER_SIZE: u16 = 512;
const THEME_TRAY_SIZE: u16 = 64;

/// The theme tokens an icon draws with: the tile fill, its outline, and the mark.
#[derive(Debug, PartialEq, Eq)]
struct ThemeIconColors {
    theme: String,
    surface: String,
    line: String,
    accent: String,
}

/// Regenerate all icon formats consumed by the desktop shell and web viewer.
pub fn run() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let icons = root.join("crates/gtl-desktop/icons");
    let web = root.join("crates/gtl-web/src/app/assets");
    let themes = theme_icon_colors(THEME_TOKENS)?;
    let default = themes
        .iter()
        .find(|colors| colors.theme == DEFAULT_THEME)
        .context("tokens.css defines no default theme")?;
    let launcher = launcher_svg(default);
    fs::write(icons.join("icon.svg"), &launcher)?;
    fs::write(web.join("app-icon.svg"), &launcher)?;
    render(&launcher, 1024)?.save(icons.join("icon.png"))?;
    // Tauri's ICNS conversion accepts at most 512px for a standard-density PNG.
    render(&launcher, 512)?.save(icons.join("icon-macos.png"))?;

    let mut ico = ico::IconDir::new(ico::ResourceType::Icon);
    for &size in ICO_SIZES {
        let image = render(&launcher, size)?;
        let entry =
            ico::IconImage::from_rgba_data(u32::from(size), u32::from(size), image.into_raw());
        ico.add_entry(ico::IconDirEntry::encode(&entry).context("encoding ICO entry")?);
    }
    ico.write(fs::File::create(icons.join("icon.ico"))?)?;

    // The desktop shell swaps these in when the viewer theme changes.
    for colors in &themes {
        let directory = icons.join("themes").join(&colors.theme);
        fs::create_dir_all(&directory)?;
        let launcher = launcher_svg(colors);
        fs::write(directory.join("launcher.svg"), &launcher)?;
        render(&launcher, THEME_LAUNCHER_SIZE)?.save(directory.join("launcher.png"))?;
        render(&tray_svg(colors), THEME_TRAY_SIZE)?.save(directory.join("tray.png"))?;
    }
    println!(
        "generated launcher SVG/PNG/ICO, web favicon SVG, and launcher and tray icons for {} themes",
        themes.len()
    );
    Ok(())
}

fn launcher_svg(colors: &ThemeIconColors) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\" width=\"24\" height=\"24\"><rect x=\"1\" y=\"1\" width=\"30\" height=\"30\" rx=\"7\" fill=\"{}\" stroke=\"{}\" stroke-width=\".5\"/><svg x=\"4\" y=\"4\" width=\"24\" height=\"24\" color=\"{}\">{}</svg></svg>",
        colors.surface,
        colors.line,
        colors.accent,
        MARK.trim()
    )
}

/// Draws the bare mark in the theme accent, without the launcher tile.
fn tray_svg(colors: &ThemeIconColors) -> String {
    MARK.replace(
        "fill=\"currentColor\"",
        &format!("color=\"{}\" fill=\"currentColor\"", colors.accent),
    )
}

/// Reads each `[data-theme="..."]` block's surface, outline, and accent tokens.
fn theme_icon_colors(tokens: &str) -> Result<Vec<ThemeIconColors>> {
    let tokens = without_comments(tokens);
    let mut themes = Vec::new();
    for rule in tokens.split('}') {
        let Some((selector, declarations)) = rule.split_once('{') else {
            continue;
        };
        let Some(theme) = selector_theme(selector) else {
            continue;
        };
        let token = |name: &str| {
            declaration(declarations, name)
                .with_context(|| format!("theme `{theme}` does not define `{name}`"))
        };
        themes.push(ThemeIconColors {
            surface: token("--surface")?,
            line: token("--line-2")?,
            accent: token("--acc")?,
            theme: theme.to_owned(),
        });
    }
    ensure!(!themes.is_empty(), "tokens.css defines no themes");
    Ok(themes)
}

fn without_comments(css: &str) -> String {
    let mut output = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        output.push_str(&rest[..start]);
        rest = rest[start..]
            .find("*/")
            .map_or("", |end| &rest[start + end + 2..]);
    }
    output.push_str(rest);
    output
}

fn selector_theme(selector: &str) -> Option<&str> {
    const PREFIX: &str = "[data-theme=\"";
    let start = selector.find(PREFIX)? + PREFIX.len();
    let length = selector[start..].find('"')?;
    Some(&selector[start..start + length])
}

fn declaration(declarations: &str, name: &str) -> Option<String> {
    declarations.split(';').find_map(|declaration| {
        let (property, value) = declaration.split_once(':')?;
        (property.trim() == name).then(|| value.trim().to_owned())
    })
}

fn render(svg: &str, size: u16) -> Result<image::RgbaImage> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).context("parsing icon SVG")?;
    // Supersampling keeps the small diagonal joins clean at tray/favicon sizes.
    let render_size = u32::from(size) * 4;
    let mut pixmap =
        tiny_skia::Pixmap::new(render_size, render_size).context("allocating icon canvas")?;
    let scale = (f32::from(size) * 4.0) / tree.size().width();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut image = image::RgbaImage::new(render_size, render_size);
    for (output, input) in image.pixels_mut().zip(pixmap.pixels()) {
        let color = input.demultiply();
        *output = image::Rgba([color.red(), color.green(), color.blue(), color.alpha()]);
    }
    Ok(image::imageops::resize(
        &image,
        u32::from(size),
        u32::from(size),
        image::imageops::FilterType::Lanczos3,
    ))
}

#[cfg(test)]
mod tests {
    use super::{THEME_TOKENS, ThemeIconColors, theme_icon_colors};

    #[test]
    fn theme_tokens_yield_every_theme_once_with_its_icon_colors() {
        let themes = theme_icon_colors(THEME_TOKENS).unwrap();
        assert_eq!(
            themes
                .iter()
                .map(|colors| colors.theme.as_str())
                .collect::<Vec<_>>(),
            ["dark", "mirage", "glacier", "graphite", "carbon"]
        );
        assert_eq!(
            themes[0],
            ThemeIconColors {
                theme: "dark".into(),
                surface: "#0d1117".into(),
                line: "#2d333b".into(),
                accent: "#b69bff".into(),
            }
        );
    }

    #[test]
    fn a_theme_without_an_icon_token_is_rejected() {
        let error =
            theme_icon_colors("[data-theme=\"plain\"] { --surface: #000; --line-2: #111; }")
                .unwrap_err();
        assert_eq!(error.to_string(), "theme `plain` does not define `--acc`");
    }
}
