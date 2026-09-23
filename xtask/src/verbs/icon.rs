//! Generates launcher, favicon, and tray assets from the compact Repository Hub SVG.

use std::{fs, path::Path};

use anyhow::{Context, Result};
use resvg::{tiny_skia, usvg};

const MARK: &str = include_str!("../../../crates/gtl-web/src/app/assets/repository-hub.svg");
const ICO_SIZES: &[u16] = &[16, 24, 32, 48, 64, 128, 256];

/// Regenerate all icon formats consumed by the desktop shell and web viewer.
pub fn run() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let icons = root.join("crates/gtl-desktop/icons");
    let web = root.join("crates/gtl-web/src/app/assets");
    // The launcher uses the default Dark palette regardless of the selected theme.
    // The tray keeps its monochrome silhouette.
    let launcher = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\" width=\"24\" height=\"24\"><rect x=\"1\" y=\"1\" width=\"30\" height=\"30\" rx=\"7\" fill=\"#0d1117\" stroke=\"#2d333b\" stroke-width=\".5\"/><svg x=\"4\" y=\"4\" width=\"24\" height=\"24\" color=\"#b69bff\">{}</svg></svg>",
        MARK.trim()
    );
    let tray = MARK.replace(
        "fill=\"currentColor\"",
        "color=\"#e8ece9\" fill=\"currentColor\"",
    );
    fs::write(icons.join("icon.svg"), &launcher)?;
    fs::write(web.join("app-icon.svg"), &launcher)?;
    fs::write(icons.join("tray.svg"), &tray)?;
    render(&launcher, 1024)?.save(icons.join("icon.png"))?;
    // Tauri's ICNS conversion accepts at most 512px for a standard-density PNG.
    render(&launcher, 512)?.save(icons.join("icon-macos.png"))?;
    render(&tray, 64)?.save(icons.join("tray.png"))?;

    let mut ico = ico::IconDir::new(ico::ResourceType::Icon);
    for &size in ICO_SIZES {
        let image = render(&launcher, size)?;
        let entry =
            ico::IconImage::from_rgba_data(u32::from(size), u32::from(size), image.into_raw());
        ico.add_entry(ico::IconDirEntry::encode(&entry).context("encoding ICO entry")?);
    }
    ico.write(fs::File::create(icons.join("icon.ico"))?)?;
    println!("generated launcher SVG/PNG/ICO, web favicon SVG, and tray SVG/PNG");
    Ok(())
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
