//! A screen laid out: its drawing steps, where it can be clicked, and the
//! room each piece of text was given, so tests can check that nothing
//! spills out of its room.

use crate::icons::Icon;
use crate::paint::{Op, Rect};
use crate::text::{Line, Style, Text, Wrap};
use crate::theme::Rgba;

/// A piece of text and the room it was given, in design pixels.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fit {
    pub what: &'static str,
    pub room: Rect,
    pub used: Rect,
}

/// The colour a piece of text is drawn in, and what it is, for the checks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ink {
    pub color: Rgba,
    pub what: &'static str,
}

impl Ink {
    pub(crate) const fn new(color: Rgba, what: &'static str) -> Ink {
        Ink { color, what }
    }
}

/// A screen laid out at one scale.
pub(crate) struct Scene<T> {
    pub ops: Vec<Op>,
    pub hits: Vec<(Rect, T)>,
    pub fits: Vec<Fit>,
}

impl<T: Copy> Scene<T> {
    /// What is under a point in design pixels: the last thing laid out
    /// there, which is the one on top.
    pub(crate) fn hit(&self, x: f32, y: f32) -> Option<T> {
        self.hits.iter().rev().find(|(r, _)| r.contains(x, y)).map(|(_, t)| *t)
    }
}

/// Lays out a scene, as a browser lays out the design at one device pixel
/// to a CSS pixel: borders and the fonts' ascents are whole pixels there.
/// The whole is then painted at its scale, as the design pages scale it.
pub(crate) struct Builder<'a, T> {
    pub text: &'a mut Text,
    pub ops: Vec<Op>,
    pub hits: Vec<(Rect, T)>,
    pub fits: Vec<Fit>,
}

impl<'a, T> Builder<'a, T> {
    pub(crate) fn new(text: &'a mut Text) -> Builder<'a, T> {
        Builder { text, ops: Vec::new(), hits: Vec::new(), fits: Vec::new() }
    }

    pub(crate) fn finish(self) -> Scene<T> {
        Scene { ops: self.ops, hits: self.hits, fits: self.fits }
    }

    pub(crate) fn push(&mut self, op: Op) {
        self.ops.push(op);
    }

    pub(crate) fn hit(&mut self, rect: Rect, target: T) {
        self.hits.push((rect, target));
    }

    pub(crate) fn icon(&mut self, icon: Icon, x: f32, y: f32, size: f32) {
        self.ops.push(Op::Icon { icon, x, y, size, opacity: 1.0 });
    }

    /// The steps laid out by `f`, apart from the rest: for a layer or a
    /// turned group.
    pub(crate) fn apart(&mut self, f: impl FnOnce(&mut Self)) -> Vec<Op> {
        let before = std::mem::take(&mut self.ops);
        f(self);
        std::mem::replace(&mut self.ops, before)
    }

    /// One line of text with its line box's top left at (`x`, `top`).
    pub(crate) fn line(&mut self, line: Line, x: f32, top: f32, ink: Ink, room: Rect) {
        self.place(line, x, top, ink, room, None);
    }

    /// One line of text, drawn only inside `clip`.
    pub(crate) fn line_clipped(&mut self, line: Line, x: f32, top: f32, ink: Ink, clip: Rect) {
        self.place(line, x, top, ink, clip, Some(clip));
    }

    fn place(&mut self, line: Line, x: f32, top: f32, ink: Ink, room: Rect, clip: Option<Rect>) {
        if line.is_empty() {
            return;
        }
        let style = line.style;
        let baseline = top + self.text.baseline(style);
        let used = Rect::new(x, top, line.width, style.line);
        // Clipped text may run past its room on purpose: only what is inside
        // shows.
        if clip.is_none() {
            self.fits.push(Fit { what: ink.what, room, used });
        }
        self.ops.push(Op::Text { line, x, baseline, color: ink.color, clip });
    }

    /// Words in lines inside `room`, from its top left: at most `max_lines`,
    /// the last cut with an ellipsis if there is more. Returns the height
    /// used.
    pub(crate) fn words(&mut self, style: Style, words: &str, room: Rect, max_lines: usize, how: Wrap, ink: Ink) -> f32 {
        let lines = self.text.wrap(style, words, room.w, max_lines, how);
        let height = lines.len() as f32 * style.line;
        for (i, line) in lines.into_iter().enumerate() {
            self.line(line, room.x, room.y + i as f32 * style.line, ink, room);
        }
        height
    }

    /// One line in `room`, cut to fit, from `x`. Returns its width.
    pub(crate) fn fit(&mut self, style: Style, words: &str, x: f32, top: f32, room: Rect, ink: Ink) -> f32 {
        let line = self.text.fit(style, words, room.right() - x);
        let width = line.width;
        self.line(line, x, top, ink, room);
        width
    }
}
