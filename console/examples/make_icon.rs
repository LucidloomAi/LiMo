//! Writes `icons/icon.ico`, the console window's icon: the design's own
//! `guardian` drawing (design/icons/svg/guardian.svg), as the guardian form's
//! launcher icon has it, on a rounded square of paper, at the sizes Windows
//! asks for. Each size is a PNG inside the icon file.
//!
//!   cargo run --example make_icon --no-default-features
//!
//! Tauri needs the file to build the exe on Windows; it is kept in the
//! repository so a build needs no drawing step.

use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};
use resvg::usvg;

const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];
const PAPER: (u8, u8, u8) = (0xf6, 0xf1, 0xe4);
const INK: (u8, u8, u8) = (0x1b, 0x26, 0x38);

fn rounded_square(size: f32, inset: f32, corner: f32) -> Option<resvg::tiny_skia::Path> {
    let (l, t, r, b) = (inset, inset, size - inset, size - inset);
    let k = corner * 0.552_284_8;
    let mut p = PathBuilder::new();
    p.move_to(l + corner, t);
    p.line_to(r - corner, t);
    p.cubic_to(r - corner + k, t, r, t + corner - k, r, t + corner);
    p.line_to(r, b - corner);
    p.cubic_to(r, b - corner + k, r - corner + k, b, r - corner, b);
    p.line_to(l + corner, b);
    p.cubic_to(l + corner - k, b, l, b - corner + k, l, b - corner);
    p.line_to(l, t + corner);
    p.cubic_to(l, t + corner - k, l + corner - k, t, l + corner, t);
    p.close();
    p.finish()
}

fn draw(tree: &usvg::Tree, size: u32) -> Vec<u8> {
    let mut pixmap = Pixmap::new(size, size).expect("a pixmap");
    let s = size as f32;
    let border = (s / 32.0).max(1.0);
    let square = rounded_square(s, border / 2.0, s * 0.22).expect("a square");
    let mut paint = Paint::default();
    paint.set_color_rgba8(PAPER.0, PAPER.1, PAPER.2, 255);
    paint.anti_alias = true;
    pixmap.fill_path(&square, &paint, FillRule::Winding, Transform::identity(), None);
    paint.set_color_rgba8(INK.0, INK.1, INK.2, 255);
    pixmap.stroke_path(&square, &paint, &Stroke { width: border, ..Stroke::default() }, Transform::identity(), None);
    // The drawing is 48 by 48; it takes four fifths of the square.
    let scale = s * 0.8 / 48.0;
    let offset = s * 0.1;
    resvg::render(tree, Transform::from_row(scale, 0.0, 0.0, scale, offset, offset), &mut pixmap.as_mut());
    pixmap.encode_png().expect("a PNG")
}

fn main() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let svg = std::fs::read_to_string(here.join("../design/icons/svg/guardian.svg")).expect("the design's guardian icon");
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("the icon reads as SVG");
    let images: Vec<(u32, Vec<u8>)> = SIZES.iter().map(|&s| (s, draw(&tree, s))).collect();

    let mut ico = Vec::new();
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, png) in &images {
        let side = if *size >= 256 { 0 } else { *size as u8 };
        ico.extend_from_slice(&[side, side, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in &images {
        ico.extend_from_slice(png);
    }
    let out = here.join("icons").join("icon.ico");
    std::fs::create_dir_all(out.parent().expect("a folder")).expect("the icons folder");
    std::fs::write(&out, &ico).expect("the icon file");
    println!("wrote icons/icon.ico: {} sizes, {} bytes", images.len(), ico.len());
}
