//! What a screen is made of, and how it is painted.
//!
//! A screen is laid out in design pixels (the 1366 by 768 of the design) as
//! a list of plain drawing steps, [`Op`]s. Painting turns them into device
//! pixels at a scale, as a browser would at that many device pixels to a
//! CSS pixel: box edges and borders land on whole pixels, text and icons
//! keep their quarter pixel places.

use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use tiny_skia::{
    FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Stroke, StrokeDash,
    Transform,
};

use crate::icons::{Icon, Icons};
use crate::text::{Clip, Line, Text};
use crate::theme::{DOT, PAPER, Rgba};

/// The LucidLoom AI logo, as the repository's README shows it.
const LOGO_PNG: &[u8] = include_bytes!("../../docs/images/lucidloom-logo.png");

/// A rectangle in design pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub(crate) const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub(crate) fn right(&self) -> f32 {
        self.x + self.w
    }

    pub(crate) fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// True when the point is inside.
    pub(crate) fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// True when `inner` lies inside, allowing for rounding.
    pub(crate) fn holds(&self, inner: &Rect) -> bool {
        const SLACK: f32 = 0.05;
        inner.x >= self.x - SLACK
            && inner.y >= self.y - SLACK
            && inner.right() <= self.right() + SLACK
            && inner.bottom() <= self.bottom() + SLACK
    }
}

/// The border of a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Border {
    /// Design pixels, already a whole number of device pixels.
    pub width: f32,
    pub color: Rgba,
    pub dashed: bool,
}

/// A box: a card, a button, a field, a chip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BoxOp {
    pub rect: Rect,
    pub radius: f32,
    pub fill: Option<Rgba>,
    pub border: Option<Border>,
    /// A hard shadow set down and to the right by this much.
    pub shadow: Option<(f32, Rgba)>,
    /// A bar of colour along the bottom inside the border, of this height:
    /// a field that takes what is typed.
    pub focus: Option<(f32, Rgba)>,
}

impl BoxOp {
    pub(crate) fn new(rect: Rect, radius: f32) -> BoxOp {
        BoxOp { rect, radius, fill: None, border: None, shadow: None, focus: None }
    }
    pub(crate) fn fill(mut self, color: Rgba) -> BoxOp {
        self.fill = Some(color);
        self
    }
    pub(crate) fn border(mut self, width: f32, color: Rgba) -> BoxOp {
        self.border = Some(Border { width, color, dashed: false });
        self
    }
    pub(crate) fn dashed(mut self, width: f32, color: Rgba) -> BoxOp {
        self.border = Some(Border { width, color, dashed: true });
        self
    }
    pub(crate) fn shadow(mut self, offset: f32, color: Rgba) -> BoxOp {
        self.shadow = Some((offset, color));
        self
    }
    pub(crate) fn focus(mut self, height: f32, color: Rgba) -> BoxOp {
        self.focus = Some((height, color));
        self
    }
}

/// A small mark drawn as plain ink strokes beside text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    /// An arrow to the right, 16 by 16: a way out opens.
    Right,
    /// An arrow to the left, 16 by 16: back.
    Left,
    /// Remove the last digit, 24 by 16.
    Delete,
}

/// One step of drawing, in design pixels.
#[derive(Clone)]
pub(crate) enum Op {
    Box(BoxOp),
    /// A straight dashed line filling `rect`: across when it is wider than
    /// tall, else down.
    Dashes { rect: Rect, color: Rgba },
    /// A line of text with its start at `x` and its baseline at `baseline`.
    Text { line: Line, x: f32, baseline: f32, color: Rgba, clip: Option<Rect> },
    Icon { icon: Icon, x: f32, y: f32, size: f32, opacity: f32 },
    Logo { x: f32, y: f32, size: f32 },
    Mark { mark: Mark, x: f32, y: f32, color: Rgba },
    Disc { cx: f32, cy: f32, r: f32, color: Rgba },
    /// The text cursor.
    Caret { x: f32, y: f32, h: f32, color: Rgba },
    /// Steps painted together and then laid down at this opacity.
    Layer { opacity: f32, bounds: Rect, ops: Vec<Op> },
    /// Steps turned by `degrees` about (`cx`, `cy`).
    Turned { degrees: f32, cx: f32, cy: f32, ops: Vec<Op> },
}

/// Where design pixels land: device = design times `k`, less the offset.
#[derive(Clone, Copy)]
struct Place {
    k: f32,
    ox: f32,
    oy: f32,
    /// For turned steps: design pixels to device pixels, in full.
    turn: Option<Transform>,
}

impl Place {
    fn x(&self, v: f32) -> f32 {
        v * self.k - self.ox
    }
    fn y(&self, v: f32) -> f32 {
        v * self.k - self.oy
    }
    /// A rectangle with its edges on whole design pixels, as a browser
    /// places a box before the design is scaled, in device pixels.
    fn snap(&self, r: &Rect) -> (f32, f32, f32, f32) {
        let x0 = r.x.round() * self.k - self.ox;
        let y0 = r.y.round() * self.k - self.oy;
        let x1 = r.right().round() * self.k - self.ox;
        let y1 = r.bottom().round() * self.k - self.oy;
        (x0, y0, x1.max(x0), y1.max(y0))
    }
}

/// The fonts, the icons and the logo, with their caches: everything a
/// picture is painted with. Each of the crate's pictures keeps one.
pub struct Painter {
    pub(crate) text: Text,
    pub(crate) icons: Icons,
    logo: Option<Pixmap>,
    logos: HashMap<u32, Rc<Pixmap>>,
}

impl fmt::Debug for Painter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Painter").finish_non_exhaustive()
    }
}

impl Default for Painter {
    fn default() -> Painter {
        Painter::new()
    }
}

impl Painter {
    /// A painter with the built in fonts, icons and logo.
    pub fn new() -> Painter {
        Painter { text: Text::new(), icons: Icons::new(), logo: Pixmap::decode_png(LOGO_PNG).ok(), logos: HashMap::new() }
    }

    /// True when every font, icon and the logo were read. They are built
    /// in, so this is only false if the crate was built wrongly.
    pub fn all_read(&mut self) -> bool {
        self.text.fonts_read() && self.logo.is_some() && Icon::ALL.iter().all(|&i| self.icons.readable(i))
    }

    /// Paints the steps at scale `k`, with the design's top left at the
    /// canvas's top left.
    pub(crate) fn paint(&mut self, canvas: &mut PixmapMut<'_>, ops: &[Op], k: f32) {
        self.paint_at(canvas, ops, Place { k, ox: 0.0, oy: 0.0, turn: None });
    }

    fn paint_at(&mut self, canvas: &mut PixmapMut<'_>, ops: &[Op], at: Place) {
        for op in ops {
            match op {
                Op::Box(b) => paint_box(canvas, b, at),
                Op::Dashes { rect, color } => paint_dashes(canvas, rect, *color, at),
                Op::Text { line, x, baseline, color, clip } => {
                    if let Some(turn) = at.turn {
                        self.text.draw_turned(canvas, line, *x, *baseline, turn, *color);
                    } else {
                        let clip = clip.map(|c| {
                            let (x0, y0, x1, y1) = at.snap(&c);
                            Clip { x0: x0 as i32, y0: y0 as i32, x1: x1 as i32, y1: y1 as i32 }
                        });
                        let y = at.y(*baseline).round();
                        self.text.draw(canvas, line, (at.x(*x), y), at.k, *color, clip);
                    }
                }
                Op::Icon { icon, x, y, size, opacity } => {
                    if let Some((pixmap, ix, iy)) = self.icons.paint(*icon, at.x(*x), at.y(*y), size * at.k) {
                        let paint = PixmapPaint { opacity: opacity.clamp(0.0, 1.0), ..PixmapPaint::default() };
                        canvas.draw_pixmap(ix, iy, Pixmap::as_ref(&pixmap), &paint, Transform::identity(), None);
                    }
                }
                Op::Logo { x, y, size } => {
                    let side = (size * at.k).round().max(1.0) as u32;
                    if let Some(logo) = self.logo_at(side) {
                        let (ix, iy) = (at.x(*x).round() as i32, at.y(*y).round() as i32);
                        canvas.draw_pixmap(ix, iy, Pixmap::as_ref(&logo), &PixmapPaint::default(), Transform::identity(), None);
                    }
                }
                Op::Mark { mark, x, y, color } => paint_mark(canvas, *mark, *x, *y, *color, at),
                Op::Disc { cx, cy, r, color } => {
                    let path = PathBuilder::from_circle(at.x(*cx), at.y(*cy), r * at.k);
                    if let Some(path) = path {
                        canvas.fill_path(&path, &solid(*color), FillRule::Winding, Transform::identity(), None);
                    }
                }
                Op::Caret { x, y, h, color } => {
                    let x0 = at.x(x.round());
                    let (y0, y1) = (at.y(y.round()), at.y((y + h).round()));
                    if let Some(r) = tiny_skia::Rect::from_ltrb(x0, y0, x0 + at.k, y1.max(y0 + 1.0)) {
                        canvas.fill_rect(r, &solid(*color), Transform::identity(), None);
                    }
                }
                Op::Layer { opacity, bounds, ops } => self.paint_layer(canvas, *opacity, bounds, ops, at),
                Op::Turned { degrees, cx, cy, ops } => {
                    let turn = Transform::from_translate(-cx, -cy)
                        .post_rotate(*degrees)
                        .post_translate(*cx, *cy)
                        .post_scale(at.k, at.k)
                        .post_translate(-at.ox, -at.oy);
                    self.paint_at(canvas, ops, Place { turn: Some(turn), ..at });
                }
            }
        }
    }

    fn paint_layer(&mut self, canvas: &mut PixmapMut<'_>, opacity: f32, bounds: &Rect, ops: &[Op], at: Place) {
        let x0 = (bounds.x * at.k).floor() - 2.0;
        let y0 = (bounds.y * at.k).floor() - 2.0;
        let x1 = (bounds.right() * at.k).ceil() + 2.0;
        let y1 = (bounds.bottom() * at.k).ceil() + 2.0;
        let (w, h) = ((x1 - x0).max(1.0) as u32, (y1 - y0).max(1.0) as u32);
        if w > 16_384 || h > 16_384 {
            return;
        }
        let Some(mut layer) = Pixmap::new(w, h) else {
            return;
        };
        self.paint_at(&mut layer.as_mut(), ops, Place { k: at.k, ox: x0, oy: y0, turn: None });
        let paint = PixmapPaint { opacity: opacity.clamp(0.0, 1.0), ..PixmapPaint::default() };
        canvas.draw_pixmap((x0 - at.ox) as i32, (y0 - at.oy) as i32, layer.as_ref(), &paint, Transform::identity(), None);
    }

    /// The logo at `side` pixels square, made smaller by averaging, so it is
    /// as clean as a browser's.
    fn logo_at(&mut self, side: u32) -> Option<Rc<Pixmap>> {
        if let Some(found) = self.logos.get(&side) {
            return Some(found.clone());
        }
        let made = Rc::new(shrink(self.logo.as_ref()?, side)?);
        if self.logos.len() > 16 {
            self.logos.clear();
        }
        self.logos.insert(side, made.clone());
        Some(made)
    }
}

/// A solid paint, smoothed at its edges.
fn solid(color: Rgba) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color.skia());
    paint.anti_alias = true;
    paint
}

/// A rectangle with rounded corners, in whatever units it is given.
pub(crate) fn rounded(x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> Option<Path> {
    if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) || x1 <= x0 || y1 <= y0 {
        return None;
    }
    let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0).max(0.0);
    let mut pb = PathBuilder::new();
    if r <= 0.0 {
        pb.push_rect(tiny_skia::Rect::from_ltrb(x0, y0, x1, y1)?);
        return pb.finish();
    }
    // A quarter circle as one cubic: the control points sit this far in
    // from the corner.
    let c = r * (1.0 - 0.552_284_7);
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.cubic_to(x1 - c, y0, x1, y0 + c, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - c, x1 - c, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.cubic_to(x0 + c, y1, x0, y1 - c, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.cubic_to(x0, y0 + c, x0 + c, y0, x0 + r, y0);
    pb.close();
    pb.finish()
}

/// The outline of a ring: the outer rounded rectangle less the inner one.
fn ring(x0: f32, y0: f32, x1: f32, y1: f32, r: f32, width: f32) -> Option<Path> {
    let outer = rounded(x0, y0, x1, y1, r)?;
    let mut pb = PathBuilder::new();
    pb.push_path(&outer);
    if let Some(inner) = rounded(x0 + width, y0 + width, x1 - width, y1 - width, (r - width).max(0.0)) {
        pb.push_path(&inner);
    }
    pb.finish()
}

fn paint_box(canvas: &mut PixmapMut<'_>, b: &BoxOp, at: Place) {
    if let Some(turn) = at.turn {
        // Turned, as the stamp is: no snapping to pixels.
        let r = &b.rect;
        let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
        if let Some(fill) = b.fill
            && let Some(path) = rounded(x0, y0, x1, y1, b.radius)
        {
            canvas.fill_path(&path, &solid(fill), FillRule::Winding, turn, None);
        }
        if let Some(border) = b.border
            && let Some(path) = ring(x0, y0, x1, y1, b.radius, border.width)
        {
            canvas.fill_path(&path, &solid(border.color), FillRule::EvenOdd, turn, None);
        }
        return;
    }
    let (x0, y0, x1, y1) = at.snap(&b.rect);
    let r = b.radius * at.k;
    let id = Transform::identity();
    if let Some((offset, color)) = b.shadow {
        let d = offset * at.k;
        if let Some(path) = rounded(x0 + d, y0 + d, x1 + d, y1 + d, r) {
            canvas.fill_path(&path, &solid(color), FillRule::Winding, id, None);
        }
    }
    if let Some(fill) = b.fill
        && let Some(path) = rounded(x0, y0, x1, y1, r)
    {
        canvas.fill_path(&path, &solid(fill), FillRule::Winding, id, None);
    }
    let bw = b.border.map(|bd| bd.width * at.k).unwrap_or(0.0);
    if let Some((height, color)) = b.focus {
        paint_focus(canvas, (x0 + bw, y0 + bw, x1 - bw, y1 - bw), (r - bw).max(0.0), height * at.k, color);
    }
    if let Some(border) = b.border {
        if border.dashed {
            if let Some(path) = rounded(x0 + bw / 2.0, y0 + bw / 2.0, x1 - bw / 2.0, y1 - bw / 2.0, (r - bw / 2.0).max(0.0)) {
                let mut stroke = Stroke { width: bw, line_cap: LineCap::Butt, line_join: LineJoin::Miter, ..Stroke::default() };
                stroke.dash = StrokeDash::new(vec![3.0 * bw, 2.0 * bw], 0.0);
                canvas.stroke_path(&path, &solid(border.color), &stroke, id, None);
            }
        } else if let Some(path) = ring(x0, y0, x1, y1, r, bw) {
            canvas.fill_path(&path, &solid(border.color), FillRule::EvenOdd, id, None);
        }
    }
}

/// The bar along the bottom inside a field's border, kept inside its
/// rounded corners.
fn paint_focus(canvas: &mut PixmapMut<'_>, inner: (f32, f32, f32, f32), r: f32, height: f32, color: Rgba) {
    let (x0, y0, x1, y1) = inner;
    let (w, h) = ((x1 - x0).max(0.0) as u32, (y1 - y0).max(0.0) as u32);
    if w == 0 || h == 0 || height <= 0.0 || w > 16_384 || h > 16_384 {
        return;
    }
    let (Some(mut local), Some(mut mask)) = (Pixmap::new(w, h), Mask::new(w, h)) else {
        return;
    };
    let Some(shape) = rounded(0.0, 0.0, w as f32, h as f32, r) else {
        return;
    };
    mask.fill_path(&shape, FillRule::Winding, true, Transform::identity());
    if let Some(bar) = tiny_skia::Rect::from_ltrb(0.0, h as f32 - height, w as f32, h as f32) {
        local.fill_rect(bar, &solid(color), Transform::identity(), Some(&mask));
    }
    canvas.draw_pixmap(x0 as i32, y0 as i32, local.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

/// A dashed rule as a browser draws a dashed border side: dashes three
/// times the width, gaps about twice, spread so that it starts and ends
/// with a dash; worked out in design pixels, then scaled.
fn paint_dashes(canvas: &mut PixmapMut<'_>, rect: &Rect, color: Rgba, at: Place) {
    let (x0, y0) = (rect.x.round(), rect.y.round());
    let (x1, y1) = (rect.right().round().max(x0), rect.bottom().round().max(y0));
    let across = x1 - x0 >= y1 - y0;
    let (length, width) = if across { (x1 - x0, (y1 - y0).max(1.0)) } else { (y1 - y0, (x1 - x0).max(1.0)) };
    if length <= 0.0 {
        return;
    }
    let dash = 3.0 * width;
    let gap = 2.0 * width;
    let count = ((length + gap) / (dash + gap)).floor().max(1.0);
    let spread = if count > 1.0 { (length - count * dash) / (count - 1.0) } else { 0.0 };
    let paint = solid(color);
    let mut pos = 0.0;
    for _ in 0..(count as u32).min(100_000) {
        let end = (pos + dash).min(length);
        let (a, b) = (pos.round(), end.round());
        let r = if across {
            tiny_skia::Rect::from_ltrb(at.x(x0 + a), at.y(y0), at.x(x0 + b), at.y(y0 + width))
        } else {
            tiny_skia::Rect::from_ltrb(at.x(x0), at.y(y0 + a), at.x(x0 + width), at.y(y0 + b))
        };
        if let Some(r) = r {
            canvas.fill_rect(r, &paint, Transform::identity(), None);
        }
        pos += dash + spread;
    }
}

fn paint_mark(canvas: &mut PixmapMut<'_>, mark: Mark, x: f32, y: f32, color: Rgba, at: Place) {
    let mut pb = PathBuilder::new();
    match mark {
        Mark::Right => {
            pb.move_to(6.0, 3.0);
            pb.line_to(11.0, 8.0);
            pb.line_to(6.0, 13.0);
        }
        Mark::Left => {
            pb.move_to(10.0, 3.0);
            pb.line_to(5.0, 8.0);
            pb.line_to(10.0, 13.0);
        }
        Mark::Delete => {
            pb.move_to(9.0, 2.0);
            pb.line_to(21.5, 2.0);
            pb.line_to(21.5, 14.0);
            pb.line_to(9.0, 14.0);
            pb.line_to(2.5, 8.0);
            pb.close();
            pb.move_to(12.5, 5.5);
            pb.line_to(17.5, 10.5);
            pb.move_to(17.5, 5.5);
            pb.line_to(12.5, 10.5);
        }
    }
    let Some(path) = pb.finish() else {
        return;
    };
    let stroke = Stroke { width: 1.8, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    let to = Transform::from_row(at.k, 0.0, 0.0, at.k, at.x(x), at.y(y));
    canvas.stroke_path(&path, &solid(color), &stroke, to, None);
}

/// The logo made `side` pixels square by averaging its pixels.
fn shrink(src: &Pixmap, side: u32) -> Option<Pixmap> {
    let mut out = Pixmap::new(side, side)?;
    let (sw, sh) = (src.width() as f32, src.height() as f32);
    let (fx, fy) = (sw / side as f32, sh / side as f32);
    let data = src.data();
    let stride = src.width() as usize * 4;
    let mut pixels = Vec::with_capacity((side * side) as usize * 4);
    for ty in 0..side {
        let (ya, yb) = (ty as f32 * fy, (ty + 1) as f32 * fy);
        for tx in 0..side {
            let (xa, xb) = (tx as f32 * fx, (tx + 1) as f32 * fx);
            let mut sum = [0.0f32; 4];
            let mut weight = 0.0;
            let mut sy = ya.floor();
            while sy < yb {
                let wy = (yb.min(sy + 1.0) - ya.max(sy)).max(0.0);
                let mut sx = xa.floor();
                while sx < xb {
                    let wx = (xb.min(sx + 1.0) - xa.max(sx)).max(0.0);
                    let at = sy as usize * stride + sx as usize * 4;
                    if let Some(px) = data.get(at..at + 4) {
                        let w = wx * wy;
                        for (s, p) in sum.iter_mut().zip(px) {
                            *s += f32::from(*p) * w;
                        }
                        weight += w;
                    }
                    sx += 1.0;
                }
                sy += 1.0;
            }
            for s in sum {
                pixels.push(if weight > 0.0 { (s / weight).round().clamp(0.0, 255.0) as u8 } else { 0 });
            }
        }
    }
    let len = out.data().len();
    out.data_mut().copy_from_slice(pixels.get(..len)?);
    Some(out)
}

/// The paper of the lock screen at scale `k`: the page colour and its faint
/// grid of dots, 18 design pixels apart, as the design's radial gradient
/// paints them.
pub(crate) fn paper(canvas: &mut PixmapMut<'_>, k: f32) {
    canvas.fill(PAPER.skia());
    let (w, h) = (canvas.width() as i32, canvas.height() as i32);
    let step = 18.0 * k;
    let (inner, outer) = (1.0 * k, 1.2 * k);
    let reach = outer.ceil() as i32 + 1;
    let alpha = DOT.a;
    let dot = [f32::from(DOT.r), f32::from(DOT.g), f32::from(DOT.b)];
    let stride = w as usize * 4;
    let data = canvas.data_mut();
    let mut cy = 9.0 * k;
    while cy - outer < h as f32 {
        let mut cx = 9.0 * k;
        while cx - outer < w as f32 {
            let (px0, py0) = ((cx - 0.5).floor() as i32 - reach, (cy - 0.5).floor() as i32 - reach);
            for py in py0.max(0)..(py0 + 2 * reach + 2).min(h) {
                for px in px0.max(0)..(px0 + 2 * reach + 2).min(w) {
                    let d = ((px as f32 + 0.5 - cx).powi(2) + (py as f32 + 0.5 - cy).powi(2)).sqrt();
                    let c = if d <= inner { 1.0 } else if d >= outer { 0.0 } else { (outer - d) / (outer - inner) };
                    if c <= 0.0 {
                        continue;
                    }
                    let a = alpha * c;
                    let at = py as usize * stride + px as usize * 4;
                    if let Some(p) = data.get_mut(at..at + 3) {
                        for (v, s) in p.iter_mut().zip(dot) {
                            *v = (s * a + f32::from(*v) * (1.0 - a)).round() as u8;
                        }
                    }
                }
            }
            cx += step;
        }
        cy += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_painter_has_everything_built_in() {
        assert!(Painter::new().all_read());
    }

    #[test]
    fn the_logo_shrinks_to_any_size() {
        let painter = Painter::new();
        let logo = painter.logo.as_ref().expect("logo");
        for side in [1, 20, 26, 57, 320] {
            let small = shrink(logo, side).expect("shrunk");
            assert_eq!(small.width(), side);
        }
    }

    #[test]
    fn rounded_rectangles_refuse_nonsense() {
        assert!(rounded(0.0, 0.0, 10.0, 10.0, 3.0).is_some());
        assert!(rounded(0.0, 0.0, 10.0, 10.0, 0.0).is_some());
        assert!(rounded(10.0, 0.0, 0.0, 10.0, 3.0).is_none());
        assert!(rounded(f32::NAN, 0.0, 10.0, 10.0, 3.0).is_none());
    }
}
