//! Renders the desktop application and tray icon from one vector-style source.
//! Writes PNG and multi-resolution ICO assets without child processes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tiny_skia::{
    BlendMode, Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point,
    SpreadMode, Stroke, Transform,
};

use crate::{
    process::{self, Status},
    verb::Verb,
};

type Rgb = (u8, u8, u8);
/// A rectangle as `(left, top, right, bottom)`.
type Rect = (f32, f32, f32, f32);

// Geometry uses a 512x512 design grid and renders at 4x before downsampling.
const RENDER: u32 = 2048;
const FINAL: u32 = 1024;
const K: f32 = 4.0;

const GREEN: Rgb = (34, 197, 94);
const RED: Rgb = (239, 68, 68);
const SLATE_TOP: Rgb = (30, 41, 59);
const SLATE_BOT: Rgb = (15, 23, 42);
const BORDER: Rgb = (71, 85, 105);
const WHITE: Rgb = (255, 255, 255);

/// Resolutions packed into the multi-resolution `.ico` (taskbar/tray/explorer needs).
const ICO_SIZES: &[u32] = &[16, 24, 32, 48, 64, 128, 256];

/// Render the icon and write `icon.png` + `icon.ico` into `crates/gtl-desktop/icons/`.
pub fn run() -> Result<()> {
    let pixmap = render()?;
    let img = to_rgba_image(&pixmap);
    let dir = icons_dir();

    let png_path = dir.join("icon.png");
    image::imageops::resize(&img, FINAL, FINAL, image::imageops::FilterType::Lanczos3)
        .save(&png_path)
        .with_context(|| format!("writing {}", png_path.display()))?;
    println!("wrote {} ({FINAL}x{FINAL})", png_path.display());

    let ico_path = dir.join("icon.ico");
    write_ico(&img, &ico_path)?;
    println!("wrote {} (.ico multi-res)", ico_path.display());

    process::result(Verb::GEN_ICON, Status::Pass);
    Ok(())
}

/// `crates/gtl-desktop/icons/`, resolved from this crate's source so the verb works from any CWD.
fn icons_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/gtl-desktop/icons")
}

/// Draw the full icon into a premultiplied render canvas.
fn render() -> Result<Pixmap> {
    let mut pixmap = Pixmap::new(RENDER, RENDER).context("allocating icon canvas")?;

    // Tile: a rounded square with a vertical slate gradient, a faint top sheen, and a border.
    let (left, top, right, bottom) = (sx(40.0), sx(40.0), sx(472.0), sx(472.0));
    let tile = rounded_rect((left, top, right, bottom), sx(96.0)).context("tile path")?;
    fill(
        &mut pixmap,
        &tile,
        &gradient(top, bottom, SLATE_TOP, 255, SLATE_BOT, 255)?,
    );
    // Sheen: white ~10% at the top, fading to nothing by mid-tile.
    fill(
        &mut pixmap,
        &tile,
        &gradient(top, top + (bottom - top) * 0.5, WHITE, 26, WHITE, 0)?,
    );
    stroke(&mut pixmap, &tile, BORDER, 140, sx(2.0));

    // Diff rows: red removed (-), green added (+).
    row(&mut pixmap, 150.0, RED, 210.0, false);
    row(&mut pixmap, 208.0, RED, 150.0, false);
    row(&mut pixmap, 284.0, GREEN, 250.0, true);
    row(&mut pixmap, 342.0, GREEN, 180.0, true);

    Ok(pixmap)
}

/// One diff row in design coordinates: highlight, gutter sign, and code line.
fn row(pixmap: &mut Pixmap, y: f32, color: Rgb, width: f32, added: bool) {
    solid(
        pixmap,
        (78.0, y - 12.0, 434.0, y + 32.0),
        10.0,
        color,
        30,
        BlendMode::Source,
    );
    let over = BlendMode::SourceOver;
    solid(
        pixmap,
        (92.0, y + 7.0, 120.0, y + 13.0),
        3.0,
        color,
        255,
        over,
    );
    if added {
        solid(
            pixmap,
            (103.0, y - 4.0, 109.0, y + 24.0),
            3.0,
            color,
            255,
            over,
        );
    }
    solid(
        pixmap,
        (150.0, y + 1.0, 150.0 + width, y + 19.0),
        9.0,
        color,
        255,
        over,
    );
}

/// Scale a design-space coordinate to the render canvas.
fn sx(value: f32) -> f32 {
    value * K
}

/// Build a rounded rectangle with cubic approximations of circular corners.
fn rounded_rect(rect: Rect, radius: f32) -> Option<tiny_skia::Path> {
    let (left, top, right, bottom) = rect;
    let radius = radius.min((right - left) / 2.0).min((bottom - top) / 2.0);
    let control_offset = radius * 0.552_284_8;
    let mut path = PathBuilder::new();
    path.move_to(left + radius, top);
    path.line_to(right - radius, top);
    path.cubic_to(
        right - radius + control_offset,
        top,
        right,
        top + radius - control_offset,
        right,
        top + radius,
    );
    path.line_to(right, bottom - radius);
    path.cubic_to(
        right,
        bottom - radius + control_offset,
        right - radius + control_offset,
        bottom,
        right - radius,
        bottom,
    );
    path.line_to(left + radius, bottom);
    path.cubic_to(
        left + radius - control_offset,
        bottom,
        left,
        bottom - radius + control_offset,
        left,
        bottom - radius,
    );
    path.line_to(left, top + radius);
    path.cubic_to(
        left,
        top + radius - control_offset,
        left + radius - control_offset,
        top,
        left + radius,
        top,
    );
    path.close();
    path.finish()
}

/// Create a vertical linear-gradient paint between two RGBA colors.
fn gradient(y0: f32, y1: f32, c0: Rgb, a0: u8, c1: Rgb, a1: u8) -> Result<Paint<'static>> {
    let shader = LinearGradient::new(
        Point::from_xy(0.0, y0),
        Point::from_xy(0.0, y1),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(c0.0, c0.1, c0.2, a0)),
            GradientStop::new(1.0, Color::from_rgba8(c1.0, c1.1, c1.2, a1)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .context("gradient endpoints must differ")?;
    Ok(Paint {
        shader,
        anti_alias: true,
        ..Paint::default()
    })
}

fn solid_paint(color: Rgb, alpha: u8) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(color.0, color.1, color.2, alpha));
    paint.anti_alias = true;
    paint
}

fn fill(pixmap: &mut Pixmap, path: &tiny_skia::Path, paint: &Paint) {
    pixmap.fill_path(path, paint, FillRule::Winding, Transform::identity(), None);
}

/// Fill a rounded rectangle given in design coordinates.
fn solid(pixmap: &mut Pixmap, rect: Rect, radius: f32, color: Rgb, alpha: u8, blend: BlendMode) {
    let (left, top, right, bottom) = rect;
    if let Some(path) = rounded_rect((sx(left), sx(top), sx(right), sx(bottom)), sx(radius)) {
        let mut paint = solid_paint(color, alpha);
        paint.blend_mode = blend;
        fill(pixmap, &path, &paint);
    }
}

fn stroke(pixmap: &mut Pixmap, path: &tiny_skia::Path, color: Rgb, alpha: u8, width: f32) {
    let mut paint = solid_paint(color, alpha);
    paint.blend_mode = BlendMode::Source;
    let stroke = Stroke {
        width,
        ..Default::default()
    };
    pixmap.stroke_path(path, &paint, &stroke, Transform::identity(), None);
}

/// Convert a premultiplied tiny-skia canvas to a straight-alpha `image` RGBA buffer.
fn to_rgba_image(pixmap: &Pixmap) -> image::RgbaImage {
    let (width, height) = (pixmap.width(), pixmap.height());
    let mut image = image::RgbaImage::new(width, height);
    for (output, input) in image.pixels_mut().zip(pixmap.pixels()) {
        let color = input.demultiply();
        *output = image::Rgba([color.red(), color.green(), color.blue(), color.alpha()]);
    }
    image
}

/// Write a multi-resolution `.ico` by Lanczos-downsampling the full render to each size.
fn write_ico(img: &image::RgbaImage, path: &Path) -> Result<()> {
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for &size in ICO_SIZES {
        let small = image::imageops::resize(img, size, size, image::imageops::FilterType::Lanczos3);
        let entry = ico::IconImage::from_rgba_data(size, size, small.into_raw());
        dir.add_entry(ico::IconDirEntry::encode(&entry).context("encoding .ico entry")?);
    }
    let file =
        std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    dir.write(file).context("writing .ico")?;
    Ok(())
}
