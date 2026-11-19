//! The time left display and its notices: small pictures that float above
//! the child's own programs, by the clock.
//!
//! Both are solid, not glass, so they can be read over any program. Their
//! pictures have clear corners and a hard shadow down and to the right, so
//! their windows must take each pixel's alpha. Neither carries the maker's
//! mark.

use std::fmt;

use limo_agent::{Show, WarningShown};
use limo_core::WarningCause;

use crate::icons::Icon;
use crate::paint::{BoxOp, Op, Painter, Rect};
use crate::picture::{Picture, Size};
use crate::scene::{Builder, Ink, Scene};
use crate::text::{Family, Style, Wrap};
use crate::theme::{
    BORDER, CARD, CORAL, INK, INK_SOFT, MARIGOLD_DEEP, PAPER2, Rgba, SHADOW, TEAL, border_width,
};
use crate::words::{self, PcClock};

const PILL_H: f32 = 34.0;
const PILL_SHADOW: f32 = 3.0;
const NOTICE_W: f32 = 340.0;
const NOTICE_SHADOW: f32 = 4.0;

const LEFT: Style = Style::new(Family::Mono, 700, 13.0, 13.0);
const LEFT_SMALL: Style = Style::new(Family::Mono, 500, 10.5, 10.5).spaced(0.1).caps();
const TITLE: Style = Style::new(Family::Display, 600, 18.0, 18.0 * 1.2).spaced(-0.005);
const WORDS: Style = Style::new(Family::Body, 400, 13.5, 13.5 * 1.4);
const WHEN: Style = Style::new(Family::Mono, 500, 11.5, 11.5).spaced(0.1).caps();
const COUNT: Style = Style::new(Family::Display, 600, 22.0, 22.0);
const OK: Style = Style::new(Family::Body, 600, 13.5, 13.5);

/// How the time left display looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PillState {
    /// Time is being counted: "1 h 12 min left".
    Counting,
    /// Time is not being counted: "1 h 12 min paused", greyed.
    Paused,
    /// 15 minutes or less: in marigold.
    Soon,
    /// The last minute, in minutes and seconds: in coral, with a coral edge.
    LastMinute,
    /// Today has no limit: "No limit today", in teal.
    NoLimit,
    /// A guardian's unlock keeps the PC open: "Open until 9:30 pm", in teal.
    Open,
}

/// What the time left display shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeLeft {
    /// The time left today, in seconds; None when today has no limit.
    pub seconds_left: Option<u64>,
    /// True while time is being counted.
    pub counting: bool,
    /// When a guardian's unlock ends, unix seconds, if one is in force.
    pub unlocked_until: Option<i64>,
    /// Now, unix seconds.
    pub now: i64,
    /// The PC's clock.
    pub pc: PcClock,
}

impl TimeLeft {
    /// The display for the engine's order, or None if it shows no time.
    pub fn from_show(show: &Show, now: i64, pc: PcClock) -> Option<TimeLeft> {
        match show {
            Show::TimeLeft { seconds_left, counting, unlocked_until, .. } => {
                Some(TimeLeft { seconds_left: *seconds_left, counting: *counting, unlocked_until: *unlocked_until, now, pc })
            }
            _ => None,
        }
    }

    /// How it looks.
    pub fn state(&self) -> PillState {
        if self.unlocked_until.is_some_and(|u| u > self.now) {
            return PillState::Open;
        }
        match self.seconds_left {
            None => PillState::NoLimit,
            Some(_) if !self.counting => PillState::Paused,
            Some(s) if s < 60 => PillState::LastMinute,
            Some(s) if s <= 15 * 60 => PillState::Soon,
            Some(_) => PillState::Counting,
        }
    }

    /// Its words: the time, and the small word after it.
    pub fn words(&self) -> (String, Option<&'static str>) {
        let left = self.seconds_left.map(words::time_left).unwrap_or_default();
        match self.state() {
            PillState::Open => {
                let until = self.unlocked_until.unwrap_or(self.now);
                (format!("Open until {}", words::until_in_sentence(self.now, until, self.pc)), None)
            }
            PillState::NoLimit => ("No limit today".to_string(), None),
            PillState::Paused => (left, Some("paused")),
            _ => (left, Some("left")),
        }
    }
}

/// A notice above the time left display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// A lock is coming: 15, 5 or 1 minute before it.
    Warning {
        /// The engine's warning.
        warning: WarningShown,
        /// Now, unix seconds: the last minute counts down.
        now: i64,
        /// The PC's clock.
        pc: PcClock,
    },
    /// A blocked program was closed.
    Closed {
        /// The service's words for it.
        text: String,
    },
}

impl Notice {
    /// The icon, the heading, the words under it, and the foot: a time, or
    /// a countdown in coral.
    fn content(&self) -> (Icon, String, String, Foot) {
        match self {
            Notice::Closed { text } => {
                (Icon::Block, "A program was closed.".to_string(), words::typographic(text), Foot::Nothing)
            }
            Notice::Warning { warning, now, pc } => {
                let last = warning.minutes <= 1;
                let icon = match warning.cause {
                    WarningCause::TimeRunsOut => Icon::Bell,
                    WarningCause::BlockedHoursStart => Icon::Moon,
                    WarningCause::UnlockEnds => Icon::Unlock,
                };
                let hint = match (warning.cause, warning.minutes) {
                    (_, 0..=1) => "Save what you are doing now.",
                    (WarningCause::TimeRunsOut, 15..) => "The PC locks when the time is used up.",
                    (WarningCause::BlockedHoursStart, _) => "Finish what you are doing.",
                    _ => "Save what you are doing.",
                };
                let foot = if last {
                    Foot::Count(words::countdown(warning.lock_at.saturating_sub(*now)))
                } else {
                    let at = words::until_in_sentence(*now, warning.lock_at, *pc);
                    match warning.cause {
                        WarningCause::TimeRunsOut => Foot::When(format!("Locks about {at}")),
                        _ => Foot::When(format!("Locks at {at}")),
                    }
                };
                (icon, words::typographic(&warning.message), hint.to_string(), foot)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Foot {
    Nothing,
    When(String),
    Count(String),
}

/// A rectangle in a picture's pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    /// From the left.
    pub x: u32,
    /// From the top.
    pub y: u32,
    /// Across.
    pub width: u32,
    /// Down.
    pub height: u32,
}

impl PixelRect {
    /// True when the point is inside.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x as f32 && y >= self.y as f32 && x < (self.x + self.width) as f32 && y < (self.y + self.height) as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Ok,
}

/// Paints the time left display and the notices, and keeps the last of
/// each so that the same again costs nothing.
pub struct Overlay {
    painter: Painter,
    pill: Option<(PillKey, Picture)>,
    notice: Option<(NoticeKey, Picture, PixelRect)>,
    /// Never shown: the slots are filled just before they are read.
    nothing: Picture,
}

impl fmt::Debug for Overlay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Overlay").finish_non_exhaustive()
    }
}

impl Default for Overlay {
    fn default() -> Overlay {
        Overlay::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PillKey {
    state: PillState,
    words: (String, Option<&'static str>),
    scale: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct NoticeKey {
    content: (Icon, String, String, Foot),
    scale: u32,
}

/// A scale kept to the crate's bounds, in thousandths, for the caches.
fn kept(scale: f32) -> (f32, u32) {
    let k = if scale.is_finite() { scale.clamp(crate::theme::MIN_SCALE, crate::theme::MAX_SCALE) } else { 1.0 };
    (k, (k * 1000.0).round() as u32)
}

impl Overlay {
    /// A painter for the time left display and its notices.
    pub fn new() -> Overlay {
        Overlay { painter: Painter::new(), pill: None, notice: None, nothing: Picture::clear(Size::new(1, 1)) }
    }

    /// The time left display at `scale` (see [`crate::scale_for`], for the
    /// monitor it is on). Its picture is its own size, with its shadow.
    pub fn time_left(&mut self, t: &TimeLeft, scale: f32) -> &Picture {
        let (k, milli) = kept(scale);
        let key = PillKey { state: t.state(), words: t.words(), scale: milli };
        if self.pill.as_ref().is_none_or(|(old, _)| *old != key) {
            let picture = self.paint_pill(&key, k);
            self.pill = Some((key, picture));
        }
        match &self.pill {
            Some((_, p)) => p,
            None => &self.nothing,
        }
    }

    fn paint_pill(&mut self, key: &PillKey, k: f32) -> Picture {
        let (scene, size) = pill_scene(&mut self.painter, key, k);
        let mut picture = Picture::clear(size);
        if let Some(mut canvas) = picture.canvas() {
            self.painter.paint(&mut canvas, &scene.ops, k);
        }
        picture
    }

    /// A notice at `scale`. Its picture is its own size, with its shadow.
    pub fn notice(&mut self, notice: &Notice, scale: f32) -> &Picture {
        let (k, milli) = kept(scale);
        let key = NoticeKey { content: notice.content(), scale: milli };
        if self.notice.as_ref().is_none_or(|(old, _, _)| *old != key) {
            let (scene, size) = notice_scene(&mut self.painter, &key, k);
            let mut picture = Picture::clear(size);
            if let Some(mut canvas) = picture.canvas() {
                self.painter.paint(&mut canvas, &scene.ops, k);
            }
            let ok = scene
                .hits
                .first()
                .map(|(r, _)| PixelRect {
                    x: (r.x * k).floor().max(0.0) as u32,
                    y: (r.y * k).floor().max(0.0) as u32,
                    width: (r.w * k).ceil().max(0.0) as u32,
                    height: (r.h * k).ceil().max(0.0) as u32,
                })
                .unwrap_or(PixelRect { x: 0, y: 0, width: 0, height: 0 });
            self.notice = Some((key, picture, ok));
        }
        match &self.notice {
            Some((_, p, _)) => p,
            None => &self.nothing,
        }
    }

    /// Where the OK button is on the notice last drawn, in its pixels.
    pub fn notice_ok(&self) -> Option<PixelRect> {
        self.notice.as_ref().map(|(_, _, ok)| *ok)
    }
}

fn pill_scene(painter: &mut Painter, key: &PillKey, k: f32) -> (Scene<Hit>, Size) {
    let bw = border_width(1.5);
    let mut b: Builder<'_, Hit> = Builder::new(&mut painter.text);
    let (icon, color, edge) = match key.state {
        PillState::Counting => (Icon::Hourglass, INK, INK),
        PillState::Paused => (Icon::Hourglass, INK_SOFT, INK),
        PillState::Soon => (Icon::Hourglass, MARIGOLD_DEEP, INK),
        PillState::LastMinute => (Icon::Hourglass, CORAL, CORAL),
        PillState::NoLimit => (Icon::Check, TEAL, INK),
        PillState::Open => (Icon::Unlock, TEAL, INK),
    };
    let style = if key.state == PillState::Paused { LEFT.weight(500) } else { LEFT };
    let main = b.text.line(style, &key.words.0);
    let small = key.words.1.map(|w| b.text.line(LEFT_SMALL, w));
    let icon_x = bw + 6.0;
    let main_x = icon_x + 24.0 + 7.0;
    let mut right = main_x + main.width;
    let small_x = right + 7.0;
    if let Some(s) = &small {
        right = small_x + s.width;
    }
    let w = right + 13.0 + bw;
    let rect = Rect::new(0.0, 0.0, w, PILL_H);
    b.push(Op::Box(BoxOp::new(rect, PILL_H / 2.0).fill(CARD).border(bw, edge).shadow(PILL_SHADOW, SHADOW)));
    let opacity = if key.state == PillState::Paused { 0.5 } else { 1.0 };
    b.push(Op::Icon { icon, x: icon_x, y: (PILL_H - 24.0) / 2.0, size: 24.0, opacity });
    b.line(main, main_x, (PILL_H - style.line) / 2.0, Ink::new(color, "time left"), rect);
    if let Some(s) = small {
        b.line(s, small_x, (PILL_H - LEFT_SMALL.line) / 2.0, Ink::new(INK_SOFT, "left"), rect);
    }
    let size = Size::new(((w + PILL_SHADOW) * k).ceil() as u32 + 1, ((PILL_H + PILL_SHADOW) * k).ceil() as u32 + 1);
    (b.finish(), size)
}

fn notice_scene(painter: &mut Painter, key: &NoticeKey, k: f32) -> (Scene<Hit>, Size) {
    let bw = border_width(1.5);
    let mut b: Builder<'_, Hit> = Builder::new(&mut painter.text);
    let (icon, title, text, foot) = &key.content;
    let x0 = bw + 14.0;
    let y0 = bw + 14.0;
    let cw = NOTICE_W - 2.0 * bw - 28.0;
    let tx = x0 + 48.0 + 12.0;
    let tw = x0 + cw - tx;
    let title_lines = b.text.wrap(TITLE, title, tw, 3, Wrap::Greedy);
    let text_lines = b.text.wrap(WORDS, text, tw, 4, Wrap::Greedy);
    let title_h = title_lines.len() as f32 * TITLE.line;
    let text_h = text_lines.len() as f32 * WORDS.line;
    let top_h = (1.0 + title_h + 5.0 + text_h).max(48.0);
    let foot_top = y0 + top_h + 12.0;
    let button_h = OK.line + 16.0 + 2.0 * bw;
    let foot_h = (34.0 - 10.0 - bw).max(button_h).max(COUNT.line);
    let total = foot_top + bw + 10.0 + foot_h + 12.0 + bw;
    let card = Rect::new(0.0, 0.0, NOTICE_W, total);
    b.push(Op::Box(BoxOp::new(card, 14.0).fill(CARD).border(bw, INK).shadow(NOTICE_SHADOW, SHADOW)));
    b.push(Op::Box(BoxOp::new(Rect::new(x0, y0, 48.0, 48.0), 10.0).fill(PAPER2)));
    b.icon(*icon, x0 + 7.0, y0 + 7.0, 34.0);
    let room = Rect::new(tx, y0, tw, top_h);
    let mut y = y0 + 1.0;
    for line in title_lines {
        b.line(line, tx, y, Ink::new(INK, "notice title"), room);
        y += TITLE.line;
    }
    y += 5.0;
    for line in text_lines {
        b.line(line, tx, y, Ink::new(INK_SOFT, "notice words"), room);
        y += WORDS.line;
    }
    b.push(Op::Dashes { rect: Rect::new(x0, foot_top, cw, bw), color: BORDER });
    let content_top = foot_top + bw + 10.0;
    let mid = content_top + foot_h / 2.0;
    // OK, at the right.
    let ok = b.text.line(OK, "OK");
    let ok_w = ok.width + 30.0 + 2.0 * bw;
    let ok_rect = Rect::new(x0 + cw - ok_w, mid - button_h / 2.0, ok_w, button_h);
    b.push(Op::Box(BoxOp::new(ok_rect, 10.0).fill(CARD).border(bw, INK).shadow(2.0, SHADOW)));
    b.line(ok, ok_rect.x + bw + 15.0, ok_rect.y + bw + 8.0, Ink::new(INK, "ok"), ok_rect);
    b.hit(ok_rect, Hit::Ok);
    let foot_room = Rect::new(x0, content_top, cw - ok_w - 10.0, foot_h);
    let (words, style, color): (&str, Style, Rgba) = match foot {
        Foot::Nothing => ("", WHEN, INK_SOFT),
        Foot::When(w) => (w, WHEN, INK_SOFT),
        Foot::Count(c) => (c, COUNT, CORAL),
    };
    if !words.is_empty() {
        b.fit(style, words, x0, mid - style.line / 2.0, foot_room, Ink::new(color, "notice foot"));
    }
    let size = Size::new(((NOTICE_W + NOTICE_SHADOW) * k).ceil() as u32 + 1, ((total + NOTICE_SHADOW) * k).ceil() as u32 + 1);
    (b.finish(), size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples::{NOW, PC, notice_states, time_left_states};

    fn pill_fits(painter: &mut Painter, t: &TimeLeft) -> Vec<crate::scene::Fit> {
        let key = PillKey { state: t.state(), words: t.words(), scale: 0 };
        pill_scene(painter, &key, 1.0).0.fits
    }

    fn notice_fits(painter: &mut Painter, n: &Notice) -> Vec<crate::scene::Fit> {
        let key = NoticeKey { content: n.content(), scale: 0 };
        notice_scene(painter, &key, 1.0).0.fits
    }

    #[test]
    fn every_state_of_the_display_has_its_look_and_words() {
        let states: Vec<(PillState, String, Option<&str>)> =
            time_left_states().iter().map(|(_, t)| (t.state(), t.words().0, t.words().1)).collect();
        assert_eq!(
            states,
            vec![
                (PillState::Counting, "1 h 12 min".to_string(), Some("left")),
                (PillState::Paused, "1 h 12 min".to_string(), Some("paused")),
                (PillState::Soon, "14 min".to_string(), Some("left")),
                (PillState::LastMinute, "0:42".to_string(), Some("left")),
                (PillState::NoLimit, "No limit today".to_string(), None),
                (PillState::Open, "Open until 10:30 pm".to_string(), None),
            ]
        );
        // An unlock that has ended is not shown; the time is.
        let ended = TimeLeft { seconds_left: Some(600), counting: true, unlocked_until: Some(NOW - 1), now: NOW, pc: PC };
        assert_eq!(ended.state(), PillState::Soon);
        let show = limo_agent::Show::TimeLeft { seconds_left: Some(42), counting: false, warning: None, unlocked_until: None };
        assert_eq!(TimeLeft::from_show(&show, NOW, PC).map(|t| t.state()), Some(PillState::Paused));
        assert_eq!(TimeLeft::from_show(&limo_agent::Show::Nothing, NOW, PC), None);
    }

    #[test]
    fn notices_say_when_or_count_down() {
        let contents: Vec<(Icon, String, Foot)> =
            notice_states().iter().map(|(_, n)| n.content()).map(|(i, _, words, foot)| (i, words, foot)).collect();
        assert_eq!(contents[0], (Icon::Bell, "The PC locks when the time is used up.".into(), Foot::When("Locks about 9:57 pm".into())));
        assert_eq!(contents[1], (Icon::Bell, "Save what you are doing.".into(), Foot::When("Locks about 9:47 pm".into())));
        assert_eq!(contents[2], (Icon::Bell, "Save what you are doing now.".into(), Foot::Count("0:42".into())));
        assert_eq!(contents[3], (Icon::Moon, "Finish what you are doing.".into(), Foot::When("Locks at 10:00 pm".into())));
        assert_eq!(contents[4], (Icon::Unlock, "Save what you are doing.".into(), Foot::When("Locks at 9:47 pm".into())));
        assert_eq!(contents[5].0, Icon::Block);
    }

    #[test]
    fn nothing_spills_out_and_every_scale_draws() {
        let mut overlay = Overlay::new();
        let long = Notice::Closed { text: "A program with a very long name indeed, longer than any real one, ".repeat(8) };
        let mut notices = notice_states();
        notices.push(("long", long));
        for scale in [0.0, 0.8, 1.0, 1.4, 2.2, 9.0, f32::NAN] {
            for (name, t) in time_left_states() {
                let p = overlay.time_left(&t, scale);
                assert!(p.width() > 20 && p.height() > 20, "{name}");
                // The corner is clear and the middle is solid.
                assert_eq!(p.pixel(0, 0).map(|px| px[3]), Some(0), "{name}");
                assert_eq!(p.pixel(p.width() / 2, p.height() / 2).map(|px| px[3]), Some(255), "{name}");
                assert!(pill_fits(&mut overlay.painter, &t).iter().all(|f| f.room.holds(&f.used)), "{name}");
            }
            for (name, n) in &notices {
                let p = overlay.notice(n, scale).clone();
                let ok = overlay.notice_ok().expect("an OK button");
                assert!(ok.x + ok.width <= p.width() && ok.y + ok.height <= p.height(), "{name}");
                assert!(ok.contains((ok.x + ok.width / 2) as f32, (ok.y + ok.height / 2) as f32));
                let fits = notice_fits(&mut overlay.painter, n);
                assert!(fits.iter().all(|f| f.room.holds(&f.used)), "{name}: {fits:?}");
            }
        }
    }

    #[test]
    fn the_same_display_again_is_not_drawn_again() {
        let mut overlay = Overlay::new();
        let (_, t) = time_left_states()[0];
        let first = overlay.time_left(&t, 1.0).rgba().as_ptr();
        assert_eq!(overlay.time_left(&t, 1.0).rgba().as_ptr(), first);
        // A second later, the same minute: the same picture.
        let later = TimeLeft { seconds_left: t.seconds_left.map(|s| s - 1), ..t };
        assert_eq!(overlay.time_left(&later, 1.0).rgba().as_ptr(), first);
    }
}
