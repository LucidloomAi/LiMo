//! Text: the theme's three fonts, shaped and kerned, broken into lines, and
//! drawn.
//!
//! The fonts are variable fonts, built into the crate. Each weight and
//! optical size is an instance of its font; shaping (HarfBuzz's rules, ported
//! to Rust) gives the glyphs and their kerned places, and the outlines of
//! the same instance are turned into exact coverage, as a browser does.
//!
//! What is cached, so that drawing the same words again costs little: each
//! instance, each shaped run of words, each glyph's outline, and each glyph's
//! coverage at a size and a quarter pixel position.

use std::collections::HashMap;
use std::rc::Rc;

use ab_glyph_rasterizer::{Rasterizer, point};
use harfrust::{Buffer, Feature, ShapeOptions, ShaperFont, Tag};
use read_fonts::model::Font;
use read_fonts::model::glyph::outline::{DrawMemory, DrawOptions, PathSink};
use read_fonts::types::GlyphId;
use tiny_skia::{PixmapMut, Transform};

use crate::theme::Rgba;

const FRAUNCES: &[u8] = include_bytes!("../fonts/fraunces/Fraunces-VariableFont_SOFT,WONK,opsz,wght.ttf");
const INSTRUMENT_SANS: &[u8] = include_bytes!("../fonts/instrument-sans/InstrumentSans-VariableFont_wdth,wght.ttf");
const SPLINE_SANS_MONO: &[u8] = include_bytes!("../fonts/spline-sans-mono/SplineSansMono-VariableFont_wght.ttf");

/// The most entries each cache keeps before it is emptied and starts again.
const CACHE_LIMIT: usize = 4096;
/// The widest or tallest glyph drawn, in pixels; anything larger is not
/// drawn rather than taking a great deal of memory.
const MAX_GLYPH_PIXELS: f32 = 2048.0;

/// The theme's three type families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Family {
    /// Fraunces: headings and large numbers.
    Display,
    /// Instrument Sans: reading text.
    Body,
    /// Spline Sans Mono: labels, stamps, times.
    Mono,
}

/// How a piece of text is set, in design pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Style {
    pub family: Family,
    pub weight: u16,
    /// The font size.
    pub size: f32,
    /// The line height.
    pub line: f32,
    /// Letter spacing, added after every character, as a browser does.
    pub spacing: f32,
    /// Shown in capitals.
    pub upper: bool,
}

impl Style {
    /// A style with no letter spacing, in lower and upper case as written.
    pub(crate) const fn new(family: Family, weight: u16, size: f32, line: f32) -> Style {
        Style { family, weight, size, line, spacing: 0.0, upper: false }
    }

    /// The same style with letter spacing, in em.
    pub(crate) const fn spaced(self, em: f32) -> Style {
        Style { spacing: em * self.size, ..self }
    }

    /// The same style in capitals.
    pub(crate) const fn caps(self) -> Style {
        Style { upper: true, ..self }
    }

    /// The same style in another weight.
    pub(crate) const fn weight(self, weight: u16) -> Style {
        Style { weight, ..self }
    }
}

/// One font at one weight and optical size.
struct Instance {
    id: u32,
    font: Font,
    upem: f32,
    /// Above the baseline, in em.
    ascent: f32,
    /// Below the baseline, in em, as a positive number.
    descent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct InstanceKey {
    family: Family,
    weight: u16,
    /// The optical size in tenths, for Fraunces; 0 for the others.
    opsz: u32,
}

/// A glyph of a shaped run, in em.
#[derive(Debug, Clone, Copy)]
struct RunGlyph {
    gid: u32,
    advance: f32,
    dx: f32,
    dy: f32,
}

/// A character, or a few that shape together, of a shaped run.
#[derive(Debug, Clone, Copy)]
struct Cluster {
    /// Its glyphs, in the run's glyph list.
    glyphs: (usize, usize),
    /// Its advance, in em.
    advance: f32,
    /// True for a space: a line may break after it, and it hangs at the
    /// end of a line.
    space: bool,
}

/// Words shaped with one instance.
struct Run {
    instance: Rc<Instance>,
    glyphs: Vec<RunGlyph>,
    clusters: Vec<Cluster>,
}

/// A glyph placed on a line, in design pixels from the line's start and
/// baseline.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Glyph {
    pub gid: u32,
    pub x: f32,
    pub y: f32,
}

/// One line of text, ready to draw.
#[derive(Clone)]
pub(crate) struct Line {
    instance: Option<Rc<Instance>>,
    pub style: Style,
    pub glyphs: Vec<Glyph>,
    /// Its width in design pixels, without the space after its last word
    /// but with the letter spacing after its last character, as a browser
    /// measures it.
    pub width: f32,
}

impl Line {
    fn empty(style: Style) -> Line {
        Line { instance: None, style, glyphs: Vec::new(), width: 0.0 }
    }

    /// True when nothing is on it.
    pub(crate) fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
}

/// How lines are broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wrap {
    /// As many words on each line as fit.
    Greedy,
    /// Lines of about the same length (CSS `text-wrap: balance`).
    Balance,
    /// As many as fit, but never one word alone on the last line when that
    /// can be helped (CSS `text-wrap: pretty`).
    Pretty,
}

/// The outline of a glyph, in font units, y up.
#[derive(Debug, Clone, Copy)]
enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Default)]
struct Collect(Vec<Seg>);

impl PathSink for Collect {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(Seg::Move(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(Seg::Line(x, y));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.push(Seg::Quad(cx0, cy0, x, y));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(Seg::Cubic(cx0, cy0, cx1, cy1, x, y));
    }
    fn close(&mut self) {
        self.0.push(Seg::Close);
    }
}

/// A glyph's coverage, 0 to 255, placed from the whole pixel left of and
/// at its origin on the baseline.
struct Mask {
    left: i32,
    top: i32,
    width: usize,
    height: usize,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MaskKey {
    instance: u32,
    gid: u32,
    /// The size in device pixels, in 64ths.
    size: u32,
    /// Where the glyph starts within its pixel, in quarters.
    quarter_x: u8,
    quarter_y: u8,
}

/// A rectangle of device pixels outside which nothing is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Clip {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

/// The fonts, and every cache of text.
pub(crate) struct Text {
    faces: [Option<Font>; 3],
    instances: HashMap<InstanceKey, Rc<Instance>>,
    runs: HashMap<(u32, u32, String), Rc<Run>>,
    outlines: HashMap<(u32, u32), Rc<Vec<Seg>>>,
    masks: HashMap<MaskKey, Rc<Mask>>,
    next_id: u32,
}

impl Text {
    /// The built in fonts, read.
    pub(crate) fn new() -> Text {
        Text {
            faces: [Font::new(FRAUNCES, 0), Font::new(INSTRUMENT_SANS, 0), Font::new(SPLINE_SANS_MONO, 0)],
            instances: HashMap::new(),
            runs: HashMap::new(),
            outlines: HashMap::new(),
            masks: HashMap::new(),
            next_id: 1,
        }
    }

    /// True when all three fonts were read.
    pub(crate) fn fonts_read(&self) -> bool {
        self.faces.iter().all(Option::is_some)
    }

    fn instance(&mut self, style: Style) -> Option<Rc<Instance>> {
        let family = style.family;
        // Fraunces has an optical size axis, which a browser sets to the
        // font size in CSS pixels. The others only vary in weight.
        let opsz = match family {
            Family::Display => (style.size.clamp(9.0, 144.0) * 10.0).round() as u32,
            _ => 0,
        };
        let key = InstanceKey { family, weight: style.weight, opsz };
        if let Some(found) = self.instances.get(&key) {
            return Some(found.clone());
        }
        let face = match family {
            Family::Display => self.faces[0].as_ref(),
            Family::Body => self.faces[1].as_ref(),
            Family::Mono => self.faces[2].as_ref(),
        }?;
        let weight = f32::from(style.weight);
        let font = match family {
            Family::Display => face.instance_builder().variations([("wght", weight), ("opsz", opsz as f32 / 10.0)]).build(),
            _ => face.instance_builder().variations([("wght", weight)]).build(),
        };
        let metrics = font.metrics();
        let upem = f32::from(metrics.units_per_em.max(1));
        let (ascent, descent) = match metrics.h_line() {
            Some(line) => (line.ascender.to_f64() as f32 / upem, -(line.descender.to_f64() as f32) / upem),
            None => (0.8, 0.2),
        };
        let instance = Rc::new(Instance { id: self.next_id, font, upem, ascent, descent });
        self.next_id = self.next_id.wrapping_add(1);
        if self.instances.len() >= CACHE_LIMIT {
            self.instances.clear();
        }
        self.instances.insert(key, instance.clone());
        Some(instance)
    }

    /// Where the baseline sits in a line box of the style's line height, in
    /// design pixels from the top of the box.
    pub(crate) fn baseline(&mut self, style: Style) -> f32 {
        // As a browser lays out a line: the ascent and the descent are whole
        // pixels, and the space left over in the line is shared, with the
        // part above rounded down to a whole pixel.
        let (a, d) = self.ascent_descent(style);
        let half = ((style.line - (a + d)) / 2.0).floor();
        half + a
    }

    /// The ascent and descent of the style's font, rounded to whole pixels
    /// as a browser does.
    pub(crate) fn ascent_descent(&mut self, style: Style) -> (f32, f32) {
        let Some(instance) = self.instance(style) else {
            return (style.size * 0.8, style.size * 0.2);
        };
        ((instance.ascent * style.size).round(), (instance.descent * style.size).round())
    }

    /// How far a line box of this style reaches above and below its
    /// baseline, in design pixels.
    pub(crate) fn extents(&mut self, style: Style) -> (f32, f32) {
        let (a, d) = self.ascent_descent(style);
        let half = (style.line - (a + d)) / 2.0;
        (a + half, d + half)
    }

    fn run(&mut self, style: Style, text: &str) -> Option<Rc<Run>> {
        let instance = self.instance(style)?;
        let text = if style.upper { text.to_uppercase() } else { text.to_string() };
        let spaced = u32::from(style.spacing != 0.0);
        let key = (instance.id, spaced, text);
        if let Some(found) = self.runs.get(&key) {
            return Some(found.clone());
        }
        let run = Rc::new(shape(&instance, &key.2, style.spacing != 0.0));
        if self.runs.len() >= CACHE_LIMIT {
            self.runs.clear();
        }
        self.runs.insert(key, run.clone());
        Some(run)
    }

    /// The width of one line of text, in design pixels.
    pub(crate) fn width(&mut self, style: Style, text: &str) -> f32 {
        self.line(style, text).width
    }

    /// One line of text, however long.
    pub(crate) fn line(&mut self, style: Style, text: &str) -> Line {
        match self.run(style, text) {
            Some(run) => place(&run, style, 0, run.clusters.len(), None),
            None => Line::empty(style),
        }
    }

    /// One line of text in at most `room` design pixels: cut after the last
    /// word that fits, with an ellipsis, when it is too long.
    pub(crate) fn fit(&mut self, style: Style, text: &str, room: f32) -> Line {
        let Some(run) = self.run(style, text) else {
            return Line::empty(style);
        };
        let whole = place(&run, style, 0, run.clusters.len(), None);
        if whole.width <= room + 0.01 {
            return whole;
        }
        self.cut(&run, style, 0, room)
    }

    /// The clusters from `start`, cut to fit `room` with an ellipsis.
    fn cut(&mut self, run: &Rc<Run>, style: Style, start: usize, room: f32) -> Line {
        let ellipsis = self.line(style, "\u{2026}");
        let room_left = room - ellipsis.width;
        let clusters = &run.clusters;
        // The furthest end, by whole clusters, that fits with the ellipsis.
        let mut end = start;
        let mut width = 0.0;
        while end < clusters.len() {
            let next = width + clusters[end].advance * style.size + style.spacing;
            if next > room_left + 0.01 {
                break;
            }
            width = next;
            end += 1;
        }
        // Back to the end of the last whole word, when there is one; a
        // single word too long for the room is cut between characters.
        let mut cut_at = end;
        if end < clusters.len() {
            let mut j = end;
            while j > start && !(!clusters[j - 1].space && (j == clusters.len() || clusters[j].space)) {
                j -= 1;
            }
            if j > start {
                cut_at = j;
            }
        }
        while cut_at > start && clusters[cut_at - 1].space {
            cut_at -= 1;
        }
        let line = place(run, style, start, cut_at, Some(&ellipsis));
        if line.width > room + 0.01 { Line::empty(style) } else { line }
    }

    /// Text broken into lines of at most `room` design pixels; at most
    /// `max_lines` of them, the last cut with an ellipsis if there is more.
    pub(crate) fn wrap(&mut self, style: Style, text: &str, room: f32, max_lines: usize, how: Wrap) -> Vec<Line> {
        if max_lines == 0 {
            return Vec::new();
        }
        let Some(run) = self.run(style, text) else {
            return Vec::new();
        };
        let mut ranges = greedy(&run, style, room);
        if ranges.len() > 1 && ranges.len() <= max_lines {
            match how {
                Wrap::Greedy => {}
                Wrap::Balance => ranges = balance(&run, style, room, ranges.len()),
                Wrap::Pretty => ranges = pretty(&run, style, room, ranges),
            }
        }
        let mut lines = Vec::new();
        for (i, &(start, end)) in ranges.iter().enumerate() {
            if i + 1 == max_lines && ranges.len() > max_lines {
                lines.push(self.cut(&run, style, start, room));
                break;
            }
            lines.push(place(&run, style, start, end, None));
        }
        lines
    }

    /// Draws a line with its start and its baseline at `at`, in device
    /// pixels, at scale `k`.
    pub(crate) fn draw(&mut self, canvas: &mut PixmapMut<'_>, line: &Line, at: (f32, f32), k: f32, color: Rgba, clip: Option<Clip>) {
        let (x, y) = at;
        let Some(instance) = line.instance.clone() else {
            return;
        };
        let px = line.style.size * k;
        if !(px.is_finite() && px > 0.0 && px <= MAX_GLYPH_PIXELS) {
            return;
        }
        for glyph in &line.glyphs {
            let gx = x + glyph.x * k;
            let gy = y + glyph.y * k;
            if !(gx.is_finite() && gy.is_finite()) || gx.abs() > 1e6 || gy.abs() > 1e6 {
                continue;
            }
            let (ix, qx) = quarter(gx);
            let (iy, qy) = quarter(gy);
            let key = MaskKey { instance: instance.id, gid: glyph.gid, size: (px * 64.0).round() as u32, quarter_x: qx, quarter_y: qy };
            let mask = match self.masks.get(&key) {
                Some(found) => found.clone(),
                None => {
                    let Some(outline) = self.outline(&instance, glyph.gid) else {
                        continue;
                    };
                    let scale = px / instance.upem;
                    let (fx, fy) = (f32::from(qx) / 4.0, f32::from(qy) / 4.0);
                    let made = Rc::new(rasterise(&outline, |ux, uy| (ux * scale + fx, -uy * scale + fy)));
                    if self.masks.len() >= CACHE_LIMIT {
                        self.masks.clear();
                    }
                    self.masks.insert(key, made.clone());
                    made
                }
            };
            blend(canvas, &mask, ix, iy, color, clip);
        }
    }

    /// Draws a line through a transform from design pixels to device
    /// pixels: for the stamp, which is turned. Not cached.
    pub(crate) fn draw_turned(&mut self, canvas: &mut PixmapMut<'_>, line: &Line, x: f32, y: f32, to_device: Transform, color: Rgba) {
        let Some(instance) = line.instance.clone() else {
            return;
        };
        let scale = line.style.size / instance.upem;
        for glyph in &line.glyphs {
            let Some(outline) = self.outline(&instance, glyph.gid) else {
                continue;
            };
            let (ox, oy) = (x + glyph.x, y + glyph.y);
            let mask = rasterise(&outline, |ux, uy| {
                let mut p = tiny_skia::Point::from_xy(ox + ux * scale, oy - uy * scale);
                to_device.map_point(&mut p);
                (p.x, p.y)
            });
            blend(canvas, &mask, 0, 0, color, None);
        }
    }

    fn outline(&mut self, instance: &Instance, gid: u32) -> Option<Rc<Vec<Seg>>> {
        let key = (instance.id, gid);
        if let Some(found) = self.outlines.get(&key) {
            return Some(found.clone());
        }
        let glyphs = instance.font.outline_glyphs();
        let glyph = glyphs.get(GlyphId::new(gid))?;
        let mut sink = Collect::default();
        glyph.draw(&DrawOptions::new(), DrawMemory::Scratch, &mut sink).ok()?;
        let outline = Rc::new(sink.0);
        if self.outlines.len() >= CACHE_LIMIT {
            self.outlines.clear();
        }
        self.outlines.insert(key, outline.clone());
        Some(outline)
    }
}

/// A device position as a whole pixel and the quarter within it.
fn quarter(v: f32) -> (i32, u8) {
    let whole = v.floor();
    let mut q = ((v - whole) * 4.0).round() as i32;
    let mut i = whole as i32;
    if q >= 4 {
        q = 0;
        i += 1;
    }
    (i, q.clamp(0, 3) as u8)
}

/// Shapes text with one instance. Letter spacing turns off the optional
/// ligatures, as it does in a browser.
fn shape(instance: &Rc<Instance>, text: &str, spaced: bool) -> Run {
    let shaper = ShaperFont::new(&instance.font);
    let mut buffer = Buffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let no_ligatures = [
        Feature::new(Tag::new(b"liga"), 0, ..),
        Feature::new(Tag::new(b"clig"), 0, ..),
        Feature::new(Tag::new(b"dlig"), 0, ..),
        Feature::new(Tag::new(b"hlig"), 0, ..),
    ];
    let features: &[Feature] = if spaced { &no_ligatures } else { &[] };
    let mut run = Run { instance: instance.clone(), glyphs: Vec::new(), clusters: Vec::new() };
    if harfrust::shape(&shaper, &mut buffer, ShapeOptions::new().features(features)).is_err() {
        return run;
    }
    let upem = instance.upem;
    let infos = buffer.glyph_infos();
    let positions = buffer.glyph_positions();
    let mut current: Option<u32> = None;
    for (i, (info, pos)) in infos.iter().zip(positions.iter()).enumerate() {
        run.glyphs.push(RunGlyph {
            gid: info.glyph_id,
            advance: pos.x_advance as f32 / upem,
            dx: pos.x_offset as f32 / upem,
            dy: pos.y_offset as f32 / upem,
        });
        if current == Some(info.cluster) {
            if let Some(last) = run.clusters.last_mut() {
                last.glyphs.1 = i + 1;
                last.advance += pos.x_advance as f32 / upem;
            }
        } else {
            current = Some(info.cluster);
            let byte = info.cluster as usize;
            let space = text.get(byte..).and_then(|rest| rest.chars().next()).is_some_and(char::is_whitespace);
            run.clusters.push(Cluster { glyphs: (i, i + 1), advance: pos.x_advance as f32 / upem, space });
        }
    }
    run
}

/// The width of clusters `start..end` in design pixels, without spaces at
/// the end.
fn span_width(run: &Run, style: Style, start: usize, end: usize) -> f32 {
    let mut end = end.min(run.clusters.len());
    while end > start && run.clusters[end - 1].space {
        end -= 1;
    }
    run.clusters[start.min(end)..end].iter().map(|c| c.advance * style.size + style.spacing).sum()
}

/// Lays clusters `start..end` out on a line, with `tail` after them.
fn place(run: &Rc<Run>, style: Style, start: usize, end: usize, tail: Option<&Line>) -> Line {
    let mut end = end.min(run.clusters.len());
    let start = start.min(end);
    while end > start && run.clusters[end - 1].space {
        end -= 1;
    }
    let mut glyphs = Vec::new();
    let mut x = 0.0;
    for cluster in &run.clusters[start..end] {
        let mut gx = x;
        for g in run.glyphs.get(cluster.glyphs.0..cluster.glyphs.1).unwrap_or(&[]) {
            glyphs.push(Glyph { gid: g.gid, x: gx + g.dx * style.size, y: -g.dy * style.size });
            gx += g.advance * style.size;
        }
        x += cluster.advance * style.size + style.spacing;
    }
    if let Some(tail) = tail {
        glyphs.extend(tail.glyphs.iter().map(|g| Glyph { gid: g.gid, x: x + g.x, y: g.y }));
        x += tail.width;
    }
    Line { instance: Some(run.instance.clone()), style, glyphs, width: x }
}

/// Breaks a run into lines of at most `room`: each line as many words as
/// fit. A word longer than the room is broken between characters.
fn greedy(run: &Run, style: Style, room: f32) -> Vec<(usize, usize)> {
    let clusters = &run.clusters;
    let n = clusters.len();
    let mut lines = Vec::new();
    let mut start = 0;
    while start < n && clusters[start].space {
        start += 1;
    }
    while start < n {
        // Add words while they fit.
        let mut end = start;
        let mut i = start;
        loop {
            // The next word: its characters, then its spaces.
            let mut word_end = i;
            while word_end < n && !clusters[word_end].space {
                word_end += 1;
            }
            let mut after = word_end;
            while after < n && clusters[after].space {
                after += 1;
            }
            if word_end == i {
                end = after;
                break;
            }
            if span_width(run, style, start, word_end) <= room + 0.01 {
                end = after;
                i = after;
                if after >= n {
                    break;
                }
            } else {
                if end == start {
                    // A word alone that is too long: as many characters as fit,
                    // and at least one.
                    let mut cut = start + 1;
                    while cut < word_end && span_width(run, style, start, cut + 1) <= room + 0.01 {
                        cut += 1;
                    }
                    end = cut;
                }
                break;
            }
        }
        if end <= start {
            end = start + 1;
        }
        lines.push((start, end));
        start = end;
        while start < n && clusters[start].space {
            start += 1;
        }
    }
    lines
}

/// The narrowest room that keeps the same number of lines, so they come
/// out about as long as each other.
fn balance(run: &Run, style: Style, room: f32, count: usize) -> Vec<(usize, usize)> {
    let mut low = 0.0f32;
    let mut high = room;
    for _ in 0..24 {
        let mid = (low + high) / 2.0;
        if greedy(run, style, mid).len() <= count {
            high = mid;
        } else {
            low = mid;
        }
    }
    greedy(run, style, high)
}

/// Greedy lines, but when the last line would be one word alone, the last
/// few lines are set a little narrower, if that brings a second word down
/// without adding a line.
fn pretty(run: &Run, style: Style, room: f32, lines: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let Some(&(last_start, last_end)) = lines.last() else {
        return lines;
    };
    let words_on_last = run.clusters[last_start..last_end].iter().filter(|c| c.space).count() + 1;
    if words_on_last > 1 {
        return lines;
    }
    let count = lines.len();
    let mut narrower = room;
    while narrower > room * 0.6 {
        narrower -= 1.0;
        let tried = greedy(run, style, narrower);
        if tried.len() > count {
            break;
        }
        if let Some(&(s, e)) = tried.last() {
            let words = run.clusters[s..e].iter().filter(|c| c.space).count() + 1;
            if words > 1 {
                return tried;
            }
        }
    }
    lines
}

/// Turns an outline into coverage, through `map` from font units to device
/// pixels.
fn rasterise(outline: &[Seg], map: impl Fn(f32, f32) -> (f32, f32)) -> Mask {
    let empty = Mask { left: 0, top: 0, width: 0, height: 0, data: Vec::new() };
    // The box around every point, control points too.
    let mut min = (f32::MAX, f32::MAX);
    let mut max = (f32::MIN, f32::MIN);
    let mut grow = |p: (f32, f32)| {
        min = (min.0.min(p.0), min.1.min(p.1));
        max = (max.0.max(p.0), max.1.max(p.1));
    };
    for seg in outline {
        match *seg {
            Seg::Move(x, y) | Seg::Line(x, y) => grow(map(x, y)),
            Seg::Quad(a, b, x, y) => {
                grow(map(a, b));
                grow(map(x, y));
            }
            Seg::Cubic(a, b, c, d, x, y) => {
                grow(map(a, b));
                grow(map(c, d));
                grow(map(x, y));
            }
            Seg::Close => {}
        }
    }
    if !(min.0.is_finite() && min.1.is_finite() && max.0.is_finite() && max.1.is_finite()) || min.0 > max.0 {
        return empty;
    }
    if max.0 - min.0 > MAX_GLYPH_PIXELS || max.1 - min.1 > MAX_GLYPH_PIXELS {
        return empty;
    }
    let left = min.0.floor() as i32;
    let top = min.1.floor() as i32;
    let width = (max.0.ceil() as i32 - left + 1).max(1) as usize;
    let height = (max.1.ceil() as i32 - top + 1).max(1) as usize;
    let (ox, oy) = (left as f32, top as f32);
    let at = |x: f32, y: f32| {
        let (dx, dy) = map(x, y);
        point(dx - ox, dy - oy)
    };
    let mut r = Rasterizer::new(width, height);
    let mut first = None;
    let mut current = point(0.0, 0.0);
    for seg in outline {
        match *seg {
            Seg::Move(x, y) => {
                if let Some(start) = first.take() {
                    r.draw_line(current, start);
                }
                current = at(x, y);
                first = Some(current);
            }
            Seg::Line(x, y) => {
                let p = at(x, y);
                r.draw_line(current, p);
                current = p;
            }
            Seg::Quad(a, b, x, y) => {
                let p = at(x, y);
                r.draw_quad(current, at(a, b), p);
                current = p;
            }
            Seg::Cubic(a, b, c, d, x, y) => {
                let p = at(x, y);
                r.draw_cubic(current, at(a, b), at(c, d), p);
                current = p;
            }
            Seg::Close => {
                if let Some(start) = first.take() {
                    r.draw_line(current, start);
                    current = start;
                }
            }
        }
    }
    if let Some(start) = first {
        r.draw_line(current, start);
    }
    let mut data = vec![0u8; width * height];
    r.for_each_pixel(|i, alpha| {
        if let Some(px) = data.get_mut(i) {
            *px = (alpha.min(1.0) * 255.0).round() as u8;
        }
    });
    Mask { left, top, width, height, data }
}

/// Lays a coverage mask in a colour over the pixels, its origin at device
/// pixel (`x`, `y`).
fn blend(canvas: &mut PixmapMut<'_>, mask: &Mask, x: i32, y: i32, color: Rgba, clip: Option<Clip>) {
    let cw = canvas.width() as i32;
    let ch = canvas.height() as i32;
    let (mut x0, mut y0, mut x1, mut y1) = (0, 0, cw, ch);
    if let Some(c) = clip {
        x0 = x0.max(c.x0);
        y0 = y0.max(c.y0);
        x1 = x1.min(c.x1);
        y1 = y1.min(c.y1);
    }
    let alpha = color.a.clamp(0.0, 1.0);
    let src = [f32::from(color.r), f32::from(color.g), f32::from(color.b), 255.0];
    let stride = cw as usize * 4;
    let data = canvas.data_mut();
    for row in 0..mask.height {
        let py = y + mask.top + row as i32;
        if py < y0 || py >= y1 {
            continue;
        }
        for col in 0..mask.width {
            let px = x + mask.left + col as i32;
            if px < x0 || px >= x1 {
                continue;
            }
            let Some(&m) = mask.data.get(row * mask.width + col) else {
                continue;
            };
            if m == 0 {
                continue;
            }
            let c = f32::from(m) / 255.0 * alpha;
            let at = py as usize * stride + px as usize * 4;
            if let Some(dst) = data.get_mut(at..at + 4) {
                for (d, s) in dst.iter_mut().zip(src.iter()) {
                    *d = (s * c + f32::from(*d) * (1.0 - c)).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: Style = Style::new(Family::Body, 400, 15.0, 21.0);

    #[test]
    fn the_three_fonts_are_built_in_and_read() {
        let mut text = Text::new();
        assert!(text.fonts_read());
        for family in [Family::Display, Family::Body, Family::Mono] {
            let line = text.line(Style::new(family, 600, 20.0, 24.0), "LiMo");
            assert_eq!(line.glyphs.len(), 4);
            assert!(line.width > 20.0);
        }
    }

    #[test]
    fn the_mono_font_advances_six_tenths_of_an_em() {
        let mut text = Text::new();
        let style = Style::new(Family::Mono, 500, 11.5, 11.5).spaced(0.12).caps();
        // "MADE BY" at 10.5 with .12em spacing is 52.92 wide in a browser.
        let w = text.width(Style { size: 10.5, ..style }.spaced(0.12), "Made by");
        assert!((w - 52.92).abs() < 0.05, "{w}");
    }

    #[test]
    fn kerning_and_weight_change_the_width() {
        let mut text = Text::new();
        let light = text.width(BODY, "Today");
        let bold = text.width(BODY.weight(700), "Today");
        assert!(bold > light);
    }

    #[test]
    fn lines_break_at_spaces_and_long_words_between_characters() {
        let mut text = Text::new();
        let lines = text.wrap(BODY, "one two three four five six seven", 80.0, 10, Wrap::Greedy);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.width <= 80.01));
        let long = "x".repeat(200);
        let lines = text.wrap(BODY, &long, 100.0, 100, Wrap::Greedy);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.width <= 100.01 && !l.is_empty()));
        assert_eq!(lines.iter().map(|l| l.glyphs.len()).sum::<usize>(), 200);
    }

    #[test]
    fn too_many_lines_end_with_an_ellipsis_inside_the_room() {
        let mut text = Text::new();
        let lines = text.wrap(BODY, &"word ".repeat(80), 120.0, 2, Wrap::Greedy);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.width <= 120.01));
        let cut = text.fit(BODY, &"word ".repeat(40), 90.0);
        assert!(cut.width <= 90.01 && !cut.is_empty());
        // Nothing fits in no room.
        assert!(text.fit(BODY, "word", 0.0).is_empty());
    }

    #[test]
    fn balanced_lines_are_closer_in_length() {
        let mut text = Text::new();
        let style = Style::new(Family::Display, 600, 33.0, 36.96).spaced(-0.012);
        let greedy = text.wrap(style, "Today's time is used up.", 335.0, 2, Wrap::Greedy);
        let balanced = text.wrap(style, "Today's time is used up.", 335.0, 2, Wrap::Balance);
        assert_eq!(greedy.len(), 2);
        assert_eq!(balanced.len(), 2);
        let spread = |l: &[Line]| (l[0].width - l[1].width).abs();
        assert!(spread(&balanced) <= spread(&greedy));
    }

    #[test]
    fn odd_text_does_not_panic() {
        let mut text = Text::new();
        for s in ["", " ", "   a   ", "\u{0}\u{1}", "\u{200b}\u{200b}", "مرحبا", "日本語のテキスト", "e\u{301}\u{301}\u{301}", "\u{1F600}"] {
            let _ = text.wrap(BODY, s, 50.0, 3, Wrap::Pretty);
            let _ = text.fit(BODY, s, 10.0);
            let _ = text.wrap(BODY, s, -5.0, 3, Wrap::Balance);
        }
    }
}
