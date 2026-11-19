//! The lock screen laid out, as `design/screens/pc.css` lays it out.
//!
//! Everything is in design pixels: the screen is laid out for its own size
//! divided by the scale, which is 1366 by 768 or close to it, and painted
//! at the scale. Each number below is the stylesheet's.

use std::fmt;

use zeroize::Zeroizing;

use crate::icons::Icon;
use crate::paint::{BoxOp, Mark, Op, Rect};
use crate::picture::Size;
use crate::scene::{Builder, Ink, Scene};
use crate::text::{Family, Style, Text, Wrap};
use crate::theme::{
    BORDER, CARD, CORAL, INK, INK_SOFT, MARIGOLD, MARIGOLD_DEEP, PAPER2, PIN_PLACEHOLDER, PLACEHOLDER, REQUEST_LINE,
    Rgba, SHADOW, WAIT_WASH, border_width,
};

use super::{ASK_CHOICES, PinChoice, Target, Way};

// The type of the lock screen.
const MARK: Style = Style::new(Family::Display, 600, 24.0, 24.0).spaced(-0.01);
const NOW: Style = Style::new(Family::Display, 600, 28.0, 28.0);
const LABEL: Style = Style::new(Family::Mono, 500, 11.5, 11.5).spaced(0.12).caps();
const STAMP: Style = Style::new(Family::Mono, 600, 13.0, 13.0).spaced(0.18).caps();
const H1: Style = Style::new(Family::Display, 600, 33.0, 33.0 * 1.12).spaced(-0.012);
const DETAIL: Style = Style::new(Family::Body, 400, 15.5, 15.5 * 1.45);
const UNTIL: Style = Style::new(Family::Display, 600, 24.0, 24.0 * 1.1);
const UNTIL_PLAIN: Style = Style::new(Family::Display, 600, 19.0, 19.0 * 1.1);
const ROW_TITLE: Style = Style::new(Family::Body, 600, 16.0, 20.0);
const ROW_SUB: Style = Style::new(Family::Body, 400, 13.5, 13.5 * 1.3);
const SMALL_STAMP: Style = Style::new(Family::Mono, 600, 10.5, 10.5).spaced(0.1).caps();
const REQUEST: Style = Style::new(Family::Body, 400, 14.0, 14.0 * 1.4);
const ALSO: Style = Style::new(Family::Body, 400, 13.5, 13.5 * 1.45);
const NONE_BOLD: Style = Style::new(Family::Display, 600, 20.0, 24.0);
const NONE_TEXT: Style = Style::new(Family::Body, 400, 15.0, 15.0 * 1.45);
const BACK: Style = Style::new(Family::Body, 600, 13.0, 13.0);
const H2: Style = Style::new(Family::Display, 600, 23.0, 23.0).spaced(-0.008);
const NOTE: Style = Style::new(Family::Body, 400, 14.5, 14.5 * 1.45);
const CHIP: Style = Style::new(Family::Mono, 500, 13.0, 13.0);
const CHIP_SMALL: Style = Style::new(Family::Mono, 500, 12.5, 12.5);
const HINT: Style = Style::new(Family::Body, 400, 12.5, 12.5);
const FIELD: Style = Style::new(Family::Body, 400, 15.0, 21.0);
const COUNT: Style = Style::new(Family::Mono, 500, 11.0, 11.0);
const BUTTON: Style = Style::new(Family::Body, 600, 15.0, 15.0);
const SMALL: Style = Style::new(Family::Body, 400, 13.5, 13.5 * 1.45);
const CELL: Style = Style::new(Family::Mono, 600, 21.0, 21.0);
const ERROR: Style = Style::new(Family::Body, 600, 13.5, 19.0);
const KEY: Style = Style::new(Family::Mono, 600, 17.0, 17.0);
const PIN_HINT: Style = Style::new(Family::Mono, 500, 13.0, 13.0);
const FOOT: Style = Style::new(Family::Mono, 500, 12.0, 12.0 * 1.4);
const MAKER_WORDS: Style = Style::new(Family::Mono, 500, 10.5, 10.5).spaced(0.12).caps();
const MAKER_NAME: Style = Style::new(Family::Body, 600, 14.0, 14.0).spaced(-0.005);
const SECOND: Style = Style::new(Family::Body, 400, 16.0, 16.0 * 1.4);

/// The card, which never changes size.
pub(crate) const CARD_W: f32 = 920.0;
pub(crate) const CARD_H: f32 = 468.0;
/// The left half of the card.
const LEFT_W: f32 = 404.0;
/// The bar at the top: 24 above, then the clock and the date.
const TOP_H: f32 = 24.0 + 28.0 + 7.0 + 11.5;
/// The line at the foot: one line of 12 at 1.4, and 24 below.
const FOOT_H: f32 = 12.0 * 1.4 + 24.0;

/// The digits typed of a code, to draw in their cells. Wiped when
/// dropped; its `Debug` hides them.
pub(crate) struct Digits(Zeroizing<Vec<u8>>);

impl Digits {
    pub(crate) fn of(typed: &str) -> Digits {
        Digits(Zeroizing::new(typed.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect()))
    }
}

impl PartialEq for Digits {
    fn eq(&self, other: &Digits) -> bool {
        *self.0 == *other.0
    }
}

impl fmt::Debug for Digits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Digits(hidden)")
    }
}

/// The newest request and how it stands.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RequestLine {
    pub text: String,
    pub stamp: String,
    pub color: Rgba,
}

/// A row with a way out.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    pub way: Way,
    pub icon: Icon,
    pub title: &'static str,
    pub sub: String,
    /// Greyed: a wait after wrong tries.
    pub off: bool,
}

/// The right half of the card.
#[derive(Debug, PartialEq)]
pub(crate) enum Right {
    Ways { request: Option<RequestLine>, wait: Option<String>, rows: Vec<Row>, also: bool },
    Nothing { icon: Icon, bold: String, text: String },
    Ask { minutes: u32, reason: String, message: Option<String>, waiting: bool },
    Code { digits: Digits, message: Option<String>, waiting: bool },
    Pin { count: usize, choice: PinChoice, message: Option<String>, waiting: bool },
}

/// Everything the lock screen shows, in words.
#[derive(Debug, PartialEq)]
pub(crate) struct View {
    /// Another screen than the main one.
    pub other: bool,
    pub clock: String,
    pub date: String,
    pub icon: Icon,
    pub stamp: &'static str,
    pub stamp_color: Rgba,
    pub eyebrow: String,
    pub title: String,
    pub detail: String,
    /// When it opens again, and whether that is words rather than a time.
    pub until: Option<(String, bool)>,
    pub right: Right,
}

impl View {
    /// What another screen shows.
    pub(crate) fn other(clock: String, date: String) -> View {
        View {
            other: true,
            clock,
            date,
            icon: Icon::Lock,
            stamp: "Locked",
            stamp_color: CORAL,
            eyebrow: String::new(),
            title: String::new(),
            detail: String::new(),
            until: None,
            right: Right::Nothing { icon: Icon::Info, bold: String::new(), text: String::new() },
        }
    }
}

type B<'a> = Builder<'a, Target>;

/// The room the card needs to be whole: itself, its shadow and a little
/// paper each side, below the bar at the top and above the line at the foot.
const FIT_W: f32 = CARD_W + 4.0 + 2.0 * 22.0;
const FIT_H: f32 = TOP_H + CARD_H + 4.0 + FOOT_H + 8.0;

/// The scale of the lock screen on a screen of this size: the design's rule
/// ([`crate::scale_for`]), unless the card would then not fit, as on a
/// screen of 640 by 480; then it is made smaller still, so that nothing is
/// ever cut off at an edge.
pub fn lock_scale(size: Size) -> f32 {
    let size = size.kept();
    let (w, h) = (size.width as f32, size.height as f32);
    crate::theme::scale_for(size.width, size.height).min(w / FIT_W).min(h / FIT_H).max(0.05)
}

/// Lays the screen out for a picture of `size` at scale `k`.
pub(crate) fn lay_out(view: &View, text: &mut Text, size: Size, k: f32) -> Scene<Target> {
    let w = size.width as f32 / k;
    let h = size.height as f32 / k;
    let mut b = Builder::new(text);
    top(&mut b, view, w);
    if view.other {
        second(&mut b, w, h);
    } else {
        let line = b.text.fit(FOOT, "Nothing was closed. Your programs are still open behind this screen.", w - 40.0);
        let x = (w - line.width) / 2.0;
        b.line(line, x, h - FOOT_H, Ink::new(INK_SOFT, "foot"), Rect::new(20.0, h - FOOT_H, w - 40.0, FOOT_H));
        let cx = (w - CARD_W) / 2.0;
        let cy = TOP_H + (h - TOP_H - FOOT_H - CARD_H) / 2.0;
        card(&mut b, view, cx, cy);
    }
    maker(&mut b, w, h);
    b.finish()
}

/// LiMo's name on the left; the time and the date on the right.
fn top(b: &mut B<'_>, view: &View, w: f32) {
    let room = Rect::new(34.0, 24.0, w - 68.0, TOP_H - 24.0);
    b.icon(Icon::Hourglass, 34.0, 24.0 + (46.5 - 30.0) / 2.0, 30.0);
    let name = b.text.line(MARK, "LiMo");
    b.line(name, 72.0, 24.0 + (46.5 - 24.0) / 2.0, Ink::new(INK, "LiMo"), room);
    let half = Rect::new(w / 2.0, 24.0, w / 2.0 - 34.0, TOP_H - 24.0);
    let clock = b.text.fit(NOW, &view.clock, half.w);
    let x = w - 34.0 - clock.width;
    b.line(clock, x, 24.0, Ink::new(INK, "clock"), half);
    let date = b.text.fit(LABEL, &view.date, half.w);
    let x = w - 34.0 - date.width;
    b.line(date, x, 24.0 + 28.0 + 7.0, Ink::new(INK_SOFT, "date"), half);
}

/// The maker's mark, bottom right: "made by", the logo and the name.
fn maker(b: &mut B<'_>, w: f32, h: f32) {
    let words = b.text.line(MAKER_WORDS, "Made by");
    let name = b.text.line(MAKER_NAME, "LucidLoom AI");
    let total = words.width + 7.0 + 2.0 + 26.0 + 7.0 + name.width;
    let right = w - 34.0;
    let top = h - 20.0 - 26.0;
    let x = right - total;
    let room = Rect::new(x, top, total, 26.0);
    let name_x = x + words.width + 7.0 + 2.0 + 26.0 + 7.0;
    b.line(words, x, top + (26.0 - 10.5) / 2.0, Ink::new(INK_SOFT, "made by"), room);
    b.push(Op::Logo { x: x + room.w - name.width - 7.0 - 26.0, y: top, size: 26.0 });
    b.line(name, name_x, top + (26.0 - 14.0) / 2.0, Ink::new(INK, "maker"), room);
}

/// The stamp, turned 4 degrees: its box and its words, with its top right
/// at (`right`, `top`).
fn stamp(b: &mut B<'_>, words: &str, color: Rgba, right: f32, top: f32) -> (f32, f32) {
    let border = border_width(2.0);
    let line = b.text.line(STAMP, words);
    let w = line.width + 11.0 + 8.0 + 2.0 * border;
    let h = 7.0 + 13.0 + 6.0 + 2.0 * border;
    let x = right - w;
    let baseline = top + border + 7.0 + b.text.baseline(STAMP);
    let ops = vec![
        Op::Box(BoxOp::new(Rect::new(x, top, w, h), 7.0).border(border, color)),
        Op::Text { line, x: x + border + 11.0, baseline, color, clip: None },
    ];
    b.push(Op::Turned { degrees: -4.0, cx: x + w / 2.0, cy: top + h / 2.0, ops });
    (w, h)
}

/// The card: why on the left, what can be done on the right.
fn card(b: &mut B<'_>, view: &View, cx: f32, cy: f32) {
    let bw = border_width(1.5);
    b.push(Op::Box(BoxOp::new(Rect::new(cx, cy, CARD_W, CARD_H), 14.0).fill(CARD).border(bw, INK).shadow(4.0, SHADOW)));
    let (ix, iy) = (cx + bw, cy + bw);
    let inner_h = CARD_H - 2.0 * bw;
    b.push(Op::Dashes { rect: Rect::new(ix + LEFT_W - bw, iy, bw, inner_h), color: BORDER });
    let left = Rect::new(ix + 36.0, iy + 30.0, LEFT_W - 36.0 - 32.0 - bw, inner_h - 30.0 - 28.0);
    why(b, view, left, bw);
    let right_w = CARD_W - 2.0 * bw - LEFT_W;
    let right = Rect::new(ix + LEFT_W + 32.0, iy + 30.0, right_w - 64.0, inner_h - 30.0 - 28.0);
    match &view.right {
        Right::Ways { request, wait, rows, also } => ways(b, right, bw, request.as_ref(), wait.as_deref(), rows, *also),
        Right::Nothing { icon, bold, text } => nothing(b, right, *icon, bold, text),
        Right::Ask { minutes, reason, message, waiting } => ask(b, right, bw, *minutes, reason, message.as_deref(), *waiting),
        Right::Code { digits, message, waiting } => code(b, right, bw, digits, message.as_deref(), *waiting),
        Right::Pin { count, choice, message, waiting } => pin(b, right, bw, *count, *choice, message.as_deref(), *waiting),
    }
}

/// The left half: the picture, the stamp, whose PC, why, and when it opens.
fn why(b: &mut B<'_>, view: &View, at: Rect, bw: f32) {
    b.icon(view.icon, at.x, at.y, 88.0);
    stamp(b, view.stamp, view.stamp_color, at.right() - 2.0, at.y + 12.0);
    let top = at.y + 88.0 + 20.0;
    // The child's name and "this PC": a long name is cut, so that "this PC"
    // always shows.
    const THIS_PC: &str = " \u{b7} this PC";
    match view.eyebrow.strip_suffix(THIS_PC) {
        Some(name) => {
            let tail = b.text.line(LABEL, THIS_PC);
            let room = Rect::new(at.x, top, at.w - tail.width, LABEL.line);
            let width = b.fit(LABEL, name, at.x, top, room, Ink::new(INK_SOFT, "eyebrow"));
            b.line(tail, at.x + width, top, Ink::new(INK_SOFT, "eyebrow"), at);
        }
        None => {
            b.fit(LABEL, &view.eyebrow, at.x, top, at, Ink::new(INK_SOFT, "eyebrow"));
        }
    }
    let title_top = top + 11.5 + 10.0;
    let until_top = at.bottom() - 64.0;
    let limit = if view.until.is_some() { until_top } else { at.bottom() };
    let title_room = Rect::new(at.x, title_top, at.w, 2.0 * H1.line);
    let title_h = b.words(H1, &view.title, title_room, 2, Wrap::Balance, Ink::new(INK, "title"));
    let detail_top = title_top + title_h + 12.0;
    let lines = ((limit - detail_top) / DETAIL.line).floor().max(0.0) as usize;
    let detail_room = Rect::new(at.x, detail_top, at.w, limit - detail_top);
    b.words(DETAIL, &view.detail, detail_room, lines, Wrap::Pretty, Ink::new(INK_SOFT, "detail"));
    if let Some((when, plain)) = &view.until {
        b.push(Op::Dashes { rect: Rect::new(at.x, until_top, at.w, bw), color: BORDER });
        let label_top = until_top + bw + 16.0;
        let room = Rect::new(at.x, until_top, at.w, 64.0);
        b.fit(LABEL, "Opens again", at.x, label_top, room, Ink::new(INK_SOFT, "opens again"));
        let (style, color) = if *plain { (UNTIL_PLAIN, INK) } else { (UNTIL, MARIGOLD_DEEP) };
        b.fit(style, when, at.x, label_top + 11.5 + 8.0, room, Ink::new(color, "until"));
    }
}

/// A small stamp beside a request: outlined mono capitals.
fn small_stamp(b: &mut B<'_>, words: &str, color: Rgba, right: f32, mid: f32, bw: f32) -> f32 {
    let line = b.text.line(SMALL_STAMP, words);
    let w = line.width + 14.0 + 2.0 * bw;
    let h = 10.5 + 7.0 + 2.0 * bw;
    let x = right - w;
    let top = mid - h / 2.0;
    b.push(Op::Box(BoxOp::new(Rect::new(x, top, w, h), 6.0).border(bw, color)));
    let room = Rect::new(x, top, w, h);
    b.line(line, x + bw + 7.0, top + bw + 4.0, Ink::new(color, "request stamp"), room);
    w
}

/// The ways out: the request made, the wait, a row for each way, and the
/// line about a guardian's phone.
fn ways(b: &mut B<'_>, at: Rect, bw: f32, request: Option<&RequestLine>, wait: Option<&str>, rows: &[Row], also: bool) {
    b.fit(LABEL, "What you can do", at.x, at.y, at, Ink::new(INK_SOFT, "label"));
    let mut y = at.y + 11.5;
    if let Some(r) = request {
        y += 14.0;
        let h = 24.0 + 16.0 + 2.0 * bw;
        b.push(Op::Box(BoxOp::new(Rect::new(at.x, y, at.w, h), 10.0).dashed(bw, REQUEST_LINE)));
        let mid = y + h / 2.0;
        b.icon(Icon::Hand, at.x + bw + 10.0, mid - 12.0, 24.0);
        let stamp_w = small_stamp(b, &r.stamp, r.color, at.right() - bw - 12.0, mid, bw);
        let tx = at.x + bw + 10.0 + 24.0 + 10.0;
        let room = Rect::new(tx, y, at.right() - bw - 12.0 - stamp_w - 10.0 - tx, h);
        b.fit(REQUEST, &r.text, tx, mid - REQUEST.line / 2.0, room, Ink::new(INK, "request"));
        y += h;
    }
    if let Some(words) = wait {
        y += 14.0;
        let tx = at.x + bw + 10.0 + 24.0 + 10.0;
        let text_w = at.right() - bw - 12.0 - tx;
        let lines = b.text.wrap(REQUEST, words, text_w, 2, Wrap::Greedy);
        let content = (lines.len() as f32 * REQUEST.line).max(24.0);
        let h = content + 16.0 + 2.0 * bw;
        b.push(Op::Box(BoxOp::new(Rect::new(at.x, y, at.w, h), 10.0).fill(WAIT_WASH).border(bw, CORAL)));
        let mid = y + h / 2.0;
        b.icon(Icon::Alert, at.x + bw + 10.0, mid - 12.0, 24.0);
        let top = mid - lines.len() as f32 * REQUEST.line / 2.0;
        let room = Rect::new(tx, y, text_w, h);
        for (i, line) in lines.into_iter().enumerate() {
            b.line(line, tx, top + i as f32 * REQUEST.line, Ink::new(INK, "wait"), room);
        }
        y += h;
    }
    y += 14.0;
    for row in rows {
        let h = 48.0 + 20.0 + 2.0 * bw;
        let rect = Rect::new(at.x, y, at.w, h);
        let ops = b.apart(|b| way_row(b, rect, bw, row));
        if row.off {
            b.push(Op::Layer { opacity: 0.5, bounds: rect, ops });
        } else {
            b.ops.extend(ops);
            b.hit(rect, Target::Way(row.way));
        }
        y += h + 14.0;
    }
    let also_top = at.bottom() - 64.0;
    if also && y - 14.0 <= also_top {
        b.push(Op::Dashes { rect: Rect::new(at.x, also_top, at.w, bw), color: BORDER });
        let content_top = also_top + bw + 16.0;
        let content_h = at.bottom() - content_top;
        let words = "A guardian who is at home can also open the PC from their own phone.";
        let lines = b.text.wrap(ALSO, words, at.w, 2, Wrap::Greedy);
        let top = content_top + (content_h - lines.len() as f32 * ALSO.line) / 2.0;
        let room = Rect::new(at.x, also_top, at.w, 64.0);
        for (i, line) in lines.into_iter().enumerate() {
            b.line(line, at.x, top + i as f32 * ALSO.line, Ink::new(INK_SOFT, "also"), room);
        }
    }
}

/// One row: the icon on its tile, the way's name and a few words, and an
/// arrow when it can be opened.
fn way_row(b: &mut B<'_>, rect: Rect, bw: f32, row: &Row) {
    let mut card = BoxOp::new(rect, 10.0).fill(CARD);
    card = if row.off { card.dashed(bw, INK) } else { card.border(bw, INK).shadow(3.0, SHADOW) };
    b.push(Op::Box(card));
    let tile = Rect::new(rect.x + bw + 10.0, rect.y + bw + 10.0, 48.0, 48.0);
    b.push(Op::Box(BoxOp::new(tile, 10.0).fill(PAPER2)));
    b.icon(row.icon, tile.x + 7.0, tile.y + 7.0, 34.0);
    let tx = tile.right() + 14.0;
    let arrow_x = rect.right() - bw - 14.0 - 16.0;
    let text_right = if row.off { rect.right() - bw - 14.0 } else { arrow_x - 14.0 };
    let top = tile.y + (48.0 - (20.0 + 2.0 + ROW_SUB.line)) / 2.0;
    let room = Rect::new(tx, rect.y, text_right - tx, rect.h);
    b.fit(ROW_TITLE, row.title, tx, top, room, Ink::new(INK, "row title"));
    b.fit(ROW_SUB, &row.sub, tx, top + 22.0, room, Ink::new(INK_SOFT, "row words"));
    if !row.off {
        b.push(Op::Mark { mark: Mark::Right, x: arrow_x, y: tile.y + 16.0, color: INK });
    }
}

/// When there is no way out: an icon and a few words, in the middle.
fn nothing(b: &mut B<'_>, at: Rect, icon: Icon, bold: &str, text: &str) {
    let tx = at.x + 36.0 + 14.0;
    let tw = at.right() - tx;
    let bold_lines = b.text.wrap(NONE_BOLD, bold, tw, 2, Wrap::Greedy);
    let text_lines = b.text.wrap(NONE_TEXT, text, tw, 6, Wrap::Greedy);
    let words_h = bold_lines.len() as f32 * NONE_BOLD.line + 6.0 + text_lines.len() as f32 * NONE_TEXT.line;
    let block = words_h.max(36.0) + 20.0;
    let top = at.y + ((at.h - block) / 2.0).max(0.0);
    b.icon(icon, at.x, top, 36.0);
    let room = Rect::new(tx, at.y, tw, at.h);
    let mut y = top;
    for line in bold_lines {
        b.line(line, tx, y, Ink::new(INK, "nothing bold"), room);
        y += NONE_BOLD.line;
    }
    y += 6.0;
    for line in text_lines {
        b.line(line, tx, y, Ink::new(INK_SOFT, "nothing words"), room);
        y += NONE_TEXT.line;
    }
}

/// The head of an open way out: Back, and its name. Returns its height.
fn head(b: &mut B<'_>, at: Rect, bw: f32, title: &str) -> f32 {
    let back = b.text.line(BACK, "Back");
    let w = bw + 6.0 + 16.0 + 3.0 + back.width + 11.0 + bw;
    let h = 16.0 + 12.0 + 2.0 * bw;
    let rect = Rect::new(at.x, at.y, w, h);
    b.push(Op::Box(BoxOp::new(rect, 8.0).fill(CARD).border(bw, INK)));
    b.push(Op::Mark { mark: Mark::Left, x: at.x + bw + 6.0, y: at.y + bw + 6.0, color: INK });
    b.line(back, at.x + bw + 6.0 + 16.0 + 3.0, at.y + bw + 6.0 + 1.5, Ink::new(INK, "back"), rect);
    b.hit(rect, Target::Back);
    let x = rect.right() + 14.0;
    let room = Rect::new(x, at.y, at.right() - x, h);
    b.fit(H2, title, x, at.y + (h - H2.line) / 2.0, room, Ink::new(INK, "panel title"));
    h
}

/// A row of chips that wraps; returns the height used. `on` is filled with
/// ink at the same size.
fn chips<T: Copy + PartialEq>(
    b: &mut B<'_>,
    at: Rect,
    bw: f32,
    small: bool,
    list: &[(T, &str)],
    on: T,
    target: impl Fn(T) -> Target,
) -> f32 {
    let (style, pad_x, pad_top, pad_bottom, gap) = if small { (CHIP_SMALL, 9.0, 8.0, 7.0, 6.0) } else { (CHIP, 12.0, 9.0, 8.0, 8.0) };
    let h = style.line + pad_top + pad_bottom + 2.0 * bw;
    let (mut x, mut y) = (at.x, at.y);
    for (value, label) in list {
        let line = b.text.line(style, label);
        let w = line.width + 2.0 * pad_x + 2.0 * bw;
        if x > at.x && x + w > at.right() + 0.01 {
            x = at.x;
            y += h + gap;
        }
        let rect = Rect::new(x, y, w, h);
        let picked = *value == on;
        b.push(Op::Box(BoxOp::new(rect, 8.0).fill(if picked { INK } else { CARD }).border(bw, INK)));
        b.line(line, x + bw + pad_x, y + bw + pad_top, Ink::new(if picked { CARD } else { INK }, "chip"), rect);
        b.hit(rect, target(*value));
        x += w + gap;
    }
    y + h - at.y
}

/// The marigold button; when its way out is waiting for an answer, it is
/// pressed in and says so. Its width is that of the longer of its two
/// labels, so it never changes size.
fn button(b: &mut B<'_>, rect: Rect, bw: f32, label: &str, waiting_label: &str, waiting: bool, target: Target) {
    let pressed = if waiting { 2.0 } else { 0.0 };
    let r = Rect::new(rect.x + pressed, rect.y + pressed, rect.w, rect.h);
    let shadow = if waiting { 1.0 } else { 3.0 };
    b.push(Op::Box(BoxOp::new(r, 10.0).fill(MARIGOLD).border(bw, INK).shadow(shadow, SHADOW)));
    let words = if waiting { waiting_label } else { label };
    let line = b.text.fit(BUTTON, words, r.w - 2.0 * bw - 16.0);
    let x = r.x + (r.w - line.width) / 2.0;
    b.line(line, x, r.y + (r.h - BUTTON.line) / 2.0, Ink::new(INK, "button"), r);
    if !waiting {
        b.hit(rect, target);
    }
}

fn button_width(b: &mut B<'_>, bw: f32, labels: [&str; 2]) -> f32 {
    let widest = labels.iter().map(|l| b.text.width(BUTTON, l)).fold(0.0, f32::max);
    widest + 32.0 + 2.0 * bw
}

/// Asking for more time: how long, why, and send.
fn ask(b: &mut B<'_>, at: Rect, bw: f32, minutes: u32, reason: &str, message: Option<&str>, waiting: bool) {
    let head_h = head(b, at, bw, "Ask for more time");
    let mut y = at.y + head_h + 18.0;
    b.fit(LABEL, "How long", at.x, y, at, Ink::new(INK_SOFT, "label"));
    y += 11.5 + 9.0;
    let list: Vec<(u32, String)> = ASK_CHOICES.iter().map(|&m| (m, crate::words::minutes(m))).collect();
    let list: Vec<(u32, &str)> = list.iter().map(|(m, s)| (*m, s.as_str())).collect();
    let chips_room = Rect::new(at.x, y, at.w, at.bottom() - y);
    y += chips(b, chips_room, bw, false, &list, minutes, Target::Minutes);
    y += 18.0;
    // "WHY" and its hint share a line: the line is as tall as both need.
    let (label_up, label_down) = b.text.extents(LABEL);
    let (hint_up, hint_down) = b.text.extents(HINT);
    let up = label_up.max(hint_up);
    let line_h = up + label_down.max(hint_down);
    let label = b.text.line(LABEL, "Why");
    let label_w = label.width;
    b.line(label, at.x, y + up - label_up, Ink::new(INK_SOFT, "label"), at);
    let hint_x = at.x + label_w + 6.0;
    b.fit(HINT, "you can leave this empty", hint_x, y + up - hint_up, at, Ink::new(INK_SOFT, "hint"));
    y += line_h + 9.0;
    field(b, Rect::new(at.x, y, at.w, 118.0), bw, reason);
    y += 118.0 + 6.0;
    let count = b.text.line(COUNT, &format!("{} / 200", reason.chars().count()));
    b.line(count.clone(), at.right() - count.width, y, Ink::new(INK_SOFT, "count"), at);
    // The hint, or the message in its place, beside the button.
    let bh = BUTTON.line + 24.0 + 2.0 * bw;
    let bwidth = button_width(b, bw, ["Send request", "Sending\u{2026}"]);
    let room_w = at.w - 18.0 - bwidth;
    let (words, style, color) = match message {
        Some(m) => (m, SMALL.weight(600), CORAL),
        None => ("A guardian can say yes, give a different amount, or say no.", SMALL, INK_SOFT),
    };
    let lines = b.text.wrap(style, words, room_w, 2, Wrap::Greedy);
    let words_h = lines.len() as f32 * style.line;
    let row_h = bh.max(words_h);
    let row_top = at.bottom() - row_h;
    let room = Rect::new(at.x, row_top, room_w, row_h);
    let top = row_top + (row_h - words_h) / 2.0;
    for (i, line) in lines.into_iter().enumerate() {
        b.line(line, at.x, top + i as f32 * style.line, Ink::new(color, "ask words"), room);
    }
    let rect = Rect::new(at.right() - bwidth, row_top + (row_h - bh) / 2.0, bwidth, bh);
    button(b, rect, bw, "Send request", "Sending\u{2026}", waiting, Target::Send);
}

/// The field for the reason: what is typed, or the example in grey, with
/// the cursor at the end. When it is longer than the field, its end shows,
/// as a field scrolls to the cursor.
fn field(b: &mut B<'_>, rect: Rect, bw: f32, reason: &str) {
    b.push(Op::Box(BoxOp::new(rect, 10.0).fill(CARD).border(bw, INK).focus(4.0, MARIGOLD)));
    b.hit(rect, Target::Reason);
    let inner = Rect::new(rect.x + bw, rect.y + bw, rect.w - 2.0 * bw, rect.h - 2.0 * bw);
    let x = inner.x + 12.0;
    let top = inner.y + 10.0;
    let width = inner.w - 24.0;
    let room_h = inner.h - 20.0;
    let (ascent, descent) = b.text.ascent_descent(FIELD);
    let caret_h = ascent + descent;
    let caret_dy = (FIELD.line - caret_h) / 2.0;
    if reason.is_empty() {
        b.push(Op::Caret { x, y: top + caret_dy, h: caret_h, color: INK });
        let line = b.text.fit(FIELD, "To finish my science project", width);
        b.line_clipped(line, x, top, Ink::new(PLACEHOLDER, "placeholder"), inner);
        return;
    }
    let lines = b.text.wrap(FIELD, reason, width, usize::MAX, Wrap::Greedy);
    let total = lines.len() as f32 * FIELD.line;
    let scroll = (total - room_h).max(0.0);
    let last = lines.len().saturating_sub(1);
    // A space typed at the end moves the cursor on, though a line never
    // ends in one.
    let trailing = reason.chars().rev().take_while(|c| *c == ' ').count() as f32;
    let space = b.text.width(FIELD, "a a") - b.text.width(FIELD, "aa");
    for (i, line) in lines.into_iter().enumerate() {
        let y = top - scroll + i as f32 * FIELD.line;
        if i == last {
            let cx = (x + line.width + trailing * space).min(x + width);
            b.push(Op::Caret { x: cx, y: y + caret_dy, h: caret_h, color: INK });
        }
        b.line_clipped(line, x, y, Ink::new(INK, "reason"), inner);
    }
}

/// The digit keys: 1 to 9, then 0 and the key that removes a digit.
fn keypad(b: &mut B<'_>, x: f32, y: f32, bw: f32) {
    let col = (216.0 - 16.0) / 3.0;
    for i in 0..12u8 {
        if i == 9 {
            continue;
        }
        let (row, column) = (f32::from(i / 3), f32::from(i % 3));
        let rect = Rect::new(x + column * (col + 8.0), y + row * 48.0, col, 40.0);
        b.push(Op::Box(BoxOp::new(rect, 9.0).fill(CARD).border(bw, INK)));
        if i == 11 {
            b.push(Op::Mark { mark: Mark::Delete, x: rect.x + (col - 24.0) / 2.0, y: rect.y + 12.0, color: INK });
            b.hit(rect, Target::Delete);
            continue;
        }
        let digit = if i == 10 { 0 } else { i + 1 };
        let line = b.text.line(KEY, &digit.to_string());
        let lx = rect.x + (col - line.width) / 2.0;
        b.line(line, lx, rect.y + (40.0 - KEY.line) / 2.0, Ink::new(INK, "key"), rect);
        b.hit(rect, Target::Digit(digit));
    }
}

/// The message under a field, in its own room, one line.
fn message_line(b: &mut B<'_>, at: Rect, y: f32, message: Option<&str>) {
    if let Some(m) = message {
        let room = Rect::new(at.x, y, at.w, 19.0);
        b.fit(ERROR, m, at.x, y, room, Ink::new(CORAL, "message"));
    }
}

/// Typing a one time code: 8 cells in two groups, the keys, and use.
fn code(b: &mut B<'_>, at: Rect, bw: f32, digits: &Digits, message: Option<&str>, waiting: bool) {
    let head_h = head(b, at, bw, "Type a one time code");
    let mut y = at.y + head_h + 14.0;
    let note = "A guardian who is away from home can read you a code. It has 8 digits.";
    y += b.words(NOTE, note, Rect::new(at.x, y, at.w, 3.0 * NOTE.line), 3, Wrap::Pretty, Ink::new(INK_SOFT, "note"));
    y += 14.0;
    let typed = digits.0.len();
    for i in 0..8usize {
        let x = at.x + i as f32 * 46.0 + if i >= 4 { 12.0 } else { 0.0 };
        let rect = Rect::new(x, y, 40.0, 48.0);
        let mut cell = BoxOp::new(rect, 8.0).fill(CARD).border(bw, INK);
        if i == typed {
            cell = cell.focus(4.0, MARIGOLD);
        }
        b.push(Op::Box(cell));
        if let Some(&d) = digits.0.get(i) {
            let line = b.text.line(CELL, &d.to_string());
            let lx = x + (40.0 - line.width) / 2.0;
            b.line(line, lx, y + (48.0 - CELL.line) / 2.0, Ink::new(INK, "digit"), rect);
        }
    }
    y += 48.0 + 7.0;
    message_line(b, at, y, message);
    y += 19.0 + 7.0;
    keypad(b, at.x, y, bw);
    let side = Rect::new(at.x + 216.0 + 22.0, y, at.w - 216.0 - 22.0, 184.0);
    let small = "A code works once, only today, and only on this PC.";
    b.words(SMALL, small, Rect::new(side.x, side.y, side.w, 3.0 * SMALL.line), 3, Wrap::Greedy, Ink::new(INK_SOFT, "code words"));
    let bh = BUTTON.line + 24.0 + 2.0 * bw;
    let rect = Rect::new(side.x, side.bottom() - bh, side.w, bh);
    button(b, rect, bw, "Use this code", "Checking\u{2026}", waiting, Target::UseCode);
}

/// A guardian typing the PIN: dots for the digits, the keys, what to give,
/// and unlock.
fn pin(b: &mut B<'_>, at: Rect, bw: f32, count: usize, choice: PinChoice, message: Option<&str>, waiting: bool) {
    let head_h = head(b, at, bw, "Guardian PIN");
    let mut y = at.y + head_h + 14.0;
    let note = "For a guardian. Type your PIN and choose what to give.";
    y += b.words(NOTE, note, Rect::new(at.x, y, at.w, 3.0 * NOTE.line), 3, Wrap::Pretty, Ink::new(INK_SOFT, "note"));
    y += 14.0;
    let rect = Rect::new(at.x, y, at.w, 48.0);
    b.push(Op::Box(BoxOp::new(rect, 10.0).fill(CARD).border(bw, INK).focus(4.0, MARIGOLD)));
    let x = at.x + bw + 14.0;
    if count == 0 {
        b.fit(PIN_HINT, "6 to 8 digits", x, y + (48.0 - PIN_HINT.line) / 2.0, rect, Ink::new(PIN_PLACEHOLDER, "pin hint"));
    }
    for i in 0..count.min(8) {
        b.push(Op::Disc { cx: x + 5.5 + i as f32 * 20.0, cy: y + 24.0, r: 5.5, color: INK });
    }
    y += 48.0 + 7.0;
    message_line(b, at, y, message);
    y += 19.0 + 7.0;
    keypad(b, at.x, y, bw);
    let side = Rect::new(at.x + 216.0 + 22.0, y, at.w - 216.0 - 22.0, 184.0);
    b.fit(LABEL, "Add time", side.x, side.y, side, Ink::new(INK_SOFT, "label"));
    let mut sy = side.y + 11.5 + 8.0;
    let add: Vec<(PinChoice, &str)> = PinChoice::ALL[..3].iter().map(|c| (*c, c.label())).collect();
    sy += chips(b, Rect::new(side.x, sy, side.w, side.bottom() - sy), bw, true, &add, choice, Target::Choice);
    sy += 14.0;
    b.fit(LABEL, "Or keep it open for", side.x, sy, side, Ink::new(INK_SOFT, "label"));
    sy += 11.5 + 8.0;
    let open: Vec<(PinChoice, &str)> = PinChoice::ALL[3..].iter().map(|c| (*c, c.label())).collect();
    chips(b, Rect::new(side.x, sy, side.w, side.bottom() - sy), bw, true, &open, choice, Target::Choice);
    let bh = BUTTON.line + 24.0 + 2.0 * bw;
    let rect = Rect::new(side.x, side.bottom() - bh, side.w, bh);
    button(b, rect, bw, "Unlock", "Checking\u{2026}", waiting, Target::Unlock);
}

/// Any screen but the main one: the lock, the stamp, and where to look.
fn second(b: &mut B<'_>, w: f32, h: f32) {
    let border = border_width(2.0);
    let stamp_h = 7.0 + 13.0 + 6.0 + 2.0 * border;
    let total = 72.0 + 14.0 + stamp_h + 14.0 + SECOND.line;
    let top = TOP_H + (h - TOP_H - FOOT_H - total) / 2.0;
    b.icon(Icon::Lock, (w - 72.0) / 2.0, top, 72.0);
    let stamp_w = b.text.width(STAMP, "Locked") + 11.0 + 8.0 + 2.0 * border;
    stamp(b, "Locked", CORAL, (w + stamp_w) / 2.0, top + 86.0);
    let room = Rect::new(20.0, top, w - 40.0, total);
    let line = b.text.fit(SECOND, "Look at the main screen.", room.w);
    let x = (w - line.width) / 2.0;
    b.line(line, x, top + 86.0 + stamp_h + 14.0, Ink::new(INK_SOFT, "second"), room);
}
