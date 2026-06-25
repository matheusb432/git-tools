//! `xtask gen-icon` — render the gtl-viewer app/tray icon (single source of truth, ported from
//! the retired `generate_icon.py`). Draws the "diff lines" mark — a dark-slate squircle holding
//! stacked code lines (red removed, green added) with a +/- gutter — using tiny-skia's analytic
//! anti-aliasing, then writes `icon.png` (1024²) and a multi-resolution `icon.ico`. The `.ico` is
//! required by tauri-build's Windows resource step. Pure compute + file I/O; no child processes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tiny_skia::{
    BlendMode, Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point,
    SpreadMode, Stroke, Transform,
};

use crate::proc;

type Rgb = (u8, u8, u8);
/// A rectangle as `(left, top, right, bottom)`.
type Rect = (f32, f32, f32, f32);

// Geometry is authored in a 512×512 design space (the approved mockup); the icon is rendered on a
// supersampled canvas and downsampled to each emitted size for crisp, smooth edges.
const DESIGN: f32 = 512.0;
const RENDER: u32 = 2048;
const FINAL: u32 = 1024;
const K: f32 = RENDER as f32 / DESIGN; // design → canvas scale (4×)

const GREEN: Rgb = (34, 197, 94);
const RED: Rgb = (239, 68, 68);
const SLATE_TOP: Rgb = (30, 41, 59);
const SLATE_BOT: Rgb = (15, 23, 42);
const BORDER: Rgb = (71, 85, 105);
const WHITE: Rgb = (255, 255, 255);

/// Resolutions packed into the multi-resolution `.ico` (taskbar/tray/explorer needs).
const ICO_SIZES: &[u32] = &[16, 24, 32, 48, 64, 128, 256];

/// Render the icon and write `icon.png` + `icon.ico` into `crates/desktop/icons/`.
pub fn run() -> Result<()> {
    let pixmap = render()?;
    let img = to_rgba_image(&pixmap);
    let dir = icons_dir();

    let png_path = dir.join("icon.png");
    image::imageops::resize(&img, FINAL, FINAL, image::imageops::FilterType::Lanczos3)
        .save(&png_path)
        .with_context(|| format!("writing {}", png_path.display()))?;
    println!("wrote {} ({FINAL}×{FINAL})", png_path.display());

    let ico_path = dir.join("icon.ico");
    write_ico(&img, &ico_path)?;
    println!("wrote {} (.ico multi-res)", ico_path.display());

    proc::result("gen-icon", "PASS");
    Ok(())
}

/// `crates/desktop/icons/`, resolved from this crate's source so the verb works from any CWD.
fn icons_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/desktop/icons")
}

/// Draw the full icon into a `RENDER`-square premultiplied canvas.
fn render() -> Result<Pixmap> {
    let mut pixmap = Pixmap::new(RENDER, RENDER).context("allocating icon canvas")?;

    // Tile: a rounded square with a vertical slate gradient, a faint top sheen, and a border.
    let (l, t, r, b) = (sx(40.0), sx(40.0), sx(472.0), sx(472.0));
    let tile = rrect((l, t, r, b), sx(96.0)).context("tile path")?;
    fill(
        &mut pixmap,
        &tile,
        gradient(t, b, SLATE_TOP, 255, SLATE_BOT, 255),
    );
    // Sheen: white ~10% at the top, fading to nothing by mid-tile.
    fill(
        &mut pixmap,
        &tile,
        gradient(t, t + (b - t) * 0.5, WHITE, 26, WHITE, 0),
    );
    stroke(&mut pixmap, &tile, BORDER, 140, sx(2.0));

    // Diff rows: red removed (-), green added (+).
    row(&mut pixmap, 150.0, RED, 210.0, false);
    row(&mut pixmap, 208.0, RED, 150.0, false);
    row(&mut pixmap, 284.0, GREEN, 250.0, true);
    row(&mut pixmap, 342.0, GREEN, 180.0, true);

    Ok(pixmap)
}

/// One diff row in design coords: a faint full-width highlight, a `+`/`-` gutter sign, the code
/// line.
fn row(pixmap: &mut Pixmap, y: f32, color: Rgb, w: f32, plus: bool) {
    // The highlight is a *semi-transparent* band (alpha 30) painted with `Source` (replace), not
    // blended — this reproduces the approved PIL look, where it reads pale over a light background.
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
    ); // gutter horizontal bar
    if plus {
        solid(
            pixmap,
            (103.0, y - 4.0, 109.0, y + 24.0),
            3.0,
            color,
            255,
            over,
        ); // gutter vertical bar
    }
    solid(
        pixmap,
        (150.0, y + 1.0, 150.0 + w, y + 19.0),
        9.0,
        color,
        255,
        over,
    ); // code line
}

/// Design-space → canvas-space scale.
fn sx(v: f32) -> f32 {
    v * K
}

/// A rounded-rectangle path (canvas coords); corners are cubic approximations of circular arcs.
fn rrect(rect: Rect, radius: f32) -> Option<tiny_skia::Path> {
    let (l, t, r, b) = rect;
    let rad = radius.min((r - l) / 2.0).min((b - t) / 2.0);
    let c = rad * 0.552_284_75; // cubic control offset for a near-circular corner
    let mut pb = PathBuilder::new();
    pb.move_to(l + rad, t);
    pb.line_to(r - rad, t);
    pb.cubic_to(r - rad + c, t, r, t + rad - c, r, t + rad);
    pb.line_to(r, b - rad);
    pb.cubic_to(r, b - rad + c, r - rad + c, b, r - rad, b);
    pb.line_to(l + rad, b);
    pb.cubic_to(l + rad - c, b, l, b - rad + c, l, b - rad);
    pb.line_to(l, t + rad);
    pb.cubic_to(l, t + rad - c, l + rad - c, t, l + rad, t);
    pb.close();
    pb.finish()
}

/// A vertical linear-gradient paint between two RGBA colors (canvas y-coords).
fn gradient(y0: f32, y1: f32, c0: Rgb, a0: u8, c1: Rgb, a1: u8) -> Paint<'static> {
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
    .expect("non-degenerate vertical gradient");
    Paint {
        shader,
        anti_alias: true,
        ..Paint::default()
    }
}

fn solid_paint(color: Rgb, a: u8) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(color.0, color.1, color.2, a));
    paint.anti_alias = true;
    paint
}

fn fill(pixmap: &mut Pixmap, path: &tiny_skia::Path, paint: Paint) {
    pixmap.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
}

/// Fill a rounded rect given in *design* coords with a solid color and blend mode.
fn solid(pixmap: &mut Pixmap, rect: Rect, radius: f32, color: Rgb, a: u8, blend: BlendMode) {
    let (l, t, r, b) = rect;
    if let Some(path) = rrect((sx(l), sx(t), sx(r), sx(b)), sx(radius)) {
        let mut paint = solid_paint(color, a);
        paint.blend_mode = blend;
        fill(pixmap, &path, paint);
    }
}

fn stroke(pixmap: &mut Pixmap, path: &tiny_skia::Path, color: Rgb, a: u8, width: f32) {
    // `Source` (replace) so the alpha-140 border stays semi-transparent like the PIL original,
    // rather than blending opaque over the slate.
    let mut paint = solid_paint(color, a);
    paint.blend_mode = BlendMode::Source;
    let stroke = Stroke {
        width,
        ..Default::default()
    };
    pixmap.stroke_path(path, &paint, &stroke, Transform::identity(), None);
}

/// Convert a premultiplied tiny-skia canvas to a straight-alpha `image` RGBA buffer.
fn to_rgba_image(pixmap: &Pixmap) -> image::RgbaImage {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut img = image::RgbaImage::new(w, h);
    for (i, px) in pixmap.pixels().iter().enumerate() {
        let c = px.demultiply();
        img.put_pixel(
            i as u32 % w,
            i as u32 / w,
            image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]),
        );
    }
    img
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
