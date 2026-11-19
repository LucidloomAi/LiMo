//! The drawn display's decisions, with no Windows code: the lock screen and
//! the time left display of `limo-screens`, joined to the helper.
//!
//! The Windows side (`win/drawn.rs`) makes the windows, puts the pictures on
//! them and hands in what the child does. Everything it decides is here, and
//! tested:
//!
//! - which of the lock's keys a Windows key is, with the text the user's own
//!   keyboard layout gives for it handed in ([`lock_key`], [`Mods`]);
//! - the PC's own clock, from its time zone and the user's time format
//!   ([`pc_clock`]);
//! - where the time left display starts, how a drag is kept on the screen,
//!   and where a notice goes ([`start_anchor`], [`dragged`], [`notice_at`]);
//! - a picture's pixels into a Windows bitmap's ([`copy_to_bgra`]);
//! - the notices waiting their turn ([`Shelf`]);
//! - and the display itself as a machine ([`DrawnCore`]): what the service
//!   orders, each way out the lock gives sent once through the [`Outbox`],
//!   with a typed PIN or code moved into a [`Secret`], and each answer handed
//!   only to the lock that asked.

#![forbid(unsafe_code)]

use limo_agent::{Show, WarningShown};
use limo_core::WarningCause;
use limo_screens::{
    Answer as LockAnswer, Clock, Input, Key, Lock, Notice, Overlay, Panel, PcClock, Picture, PixelRect, Size, TimeLeft,
    WayOut,
};

use crate::helper::{Outbox, ShowNow};
use crate::protocol::{Answer, Secret};

/// The Windows virtual key codes the drawn display reads.
pub mod vk {
    /// Backspace.
    pub const BACK: u32 = 0x08;
    /// Enter, on the main keys and on the number pad.
    pub const RETURN: u32 = 0x0D;
    /// Either Shift.
    pub const SHIFT: u32 = 0x10;
    /// Either Ctrl.
    pub const CONTROL: u32 = 0x11;
    /// Either Alt.
    pub const MENU: u32 = 0x12;
    /// Caps Lock.
    pub const CAPITAL: u32 = 0x14;
    /// Escape.
    pub const ESCAPE: u32 = 0x1B;
    /// The 0 of the top row.
    pub const DIGIT_0: u32 = 0x30;
    /// The 9 of the top row.
    pub const DIGIT_9: u32 = 0x39;
    /// The left Windows key.
    pub const LWIN: u32 = 0x5B;
    /// The right Windows key.
    pub const RWIN: u32 = 0x5C;
    /// The 0 of the number pad.
    pub const NUMPAD_0: u32 = 0x60;
    /// The 9 of the number pad.
    pub const NUMPAD_9: u32 = 0x69;
    /// The left Shift.
    pub const LSHIFT: u32 = 0xA0;
    /// The right Shift.
    pub const RSHIFT: u32 = 0xA1;
    /// The left Ctrl.
    pub const LCONTROL: u32 = 0xA2;
    /// The right Ctrl.
    pub const RCONTROL: u32 = 0xA3;
    /// The left Alt.
    pub const LMENU: u32 = 0xA4;
    /// The right Alt, which is AltGr on many layouts.
    pub const RMENU: u32 = 0xA5;
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

/// Which modifier keys are held, as the keyboard hook saw them go down and
/// up. While a lock is up the hook keeps every key from Windows, so Windows'
/// own idea of which are held is not kept up to date; this is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    shift: [bool; 2],
    ctrl: [bool; 2],
    alt: [bool; 2],
    windows: [bool; 2],
}

impl Mods {
    /// A key went down or up.
    pub fn note(&mut self, vk: u32, down: bool) {
        let held = match vk {
            vk::LSHIFT | vk::SHIFT => &mut self.shift[0],
            vk::RSHIFT => &mut self.shift[1],
            vk::LCONTROL | vk::CONTROL => &mut self.ctrl[0],
            vk::RCONTROL => &mut self.ctrl[1],
            vk::LMENU | vk::MENU => &mut self.alt[0],
            vk::RMENU => &mut self.alt[1],
            vk::LWIN => &mut self.windows[0],
            vk::RWIN => &mut self.windows[1],
            _ => return,
        };
        *held = down;
    }

    /// True while a Shift is held.
    pub fn shift(&self) -> bool {
        self.shift[0] || self.shift[1]
    }

    /// True while a Ctrl is held.
    pub fn ctrl(&self) -> bool {
        self.ctrl[0] || self.ctrl[1]
    }

    /// True while an Alt is held.
    pub fn alt(&self) -> bool {
        self.alt[0] || self.alt[1]
    }

    /// True while a Windows key is held.
    pub fn windows(&self) -> bool {
        self.windows[0] || self.windows[1]
    }

    /// True when a key pressed now types text: not while Ctrl or Alt is
    /// held alone (that is a shortcut), nor with the Windows key. With both,
    /// as AltGr sends them, it does.
    pub fn types_text(&self) -> bool {
        !self.windows() && self.ctrl() == self.alt()
    }

    /// The keyboard state Windows' `ToUnicodeEx` reads to turn a key into
    /// text with the user's own layout: the held modifiers, each side and
    /// both together, and Caps Lock when it is on.
    pub fn keyboard_state(&self, caps_lock: bool) -> [u8; 256] {
        const DOWN: u8 = 0x80;
        let mut state = [0u8; 256];
        let pairs = [
            (self.shift, vk::SHIFT, vk::LSHIFT, vk::RSHIFT),
            (self.ctrl, vk::CONTROL, vk::LCONTROL, vk::RCONTROL),
            (self.alt, vk::MENU, vk::LMENU, vk::RMENU),
        ];
        for (held, either, left, right) in pairs {
            for (on, key) in [(held[0] || held[1], either), (held[0], left), (held[1], right)] {
                if on && let Some(slot) = state.get_mut(key as usize) {
                    *slot = DOWN;
                }
            }
        }
        if caps_lock && let Some(slot) = state.get_mut(vk::CAPITAL as usize) {
            *slot = 0x01;
        }
        state
    }
}

/// The lock's key for a Windows key going down, or None when the lock does
/// not take it. Backspace, Enter and Escape always. While the ask panel is
/// open (`text_wanted`), the text the user's own keyboard layout gives for
/// the key (`text`), whatever it is. Otherwise the digits of the top row and
/// of the number pad, whatever the layout and Shift would make of them, so a
/// PIN or a code is typed the same on every keyboard.
pub fn lock_key(vk: u32, text: Option<char>, text_wanted: bool) -> Option<Key> {
    match vk {
        vk::BACK => Some(Key::Backspace),
        vk::RETURN => Some(Key::Enter),
        vk::ESCAPE => Some(Key::Escape),
        _ if text_wanted => text.filter(|c| !c.is_control()).map(Key::Char),
        vk::DIGIT_0..=vk::DIGIT_9 => char::from_digit(vk - vk::DIGIT_0, 10).map(Key::Char),
        vk::NUMPAD_0..=vk::NUMPAD_9 => char::from_digit(vk - vk::NUMPAD_0, 10).map(Key::Char),
        _ => None,
    }
}

/// The first character of the UTF-16 text Windows gave for a key, if it
/// gave a whole one.
pub fn first_char(units: &[u16]) -> Option<char> {
    char::decode_utf16(units.iter().copied()).next()?.ok()
}

/// A point from a mouse message's `lParam`: the two signed 16 bit halves,
/// in pixels of the window it came to.
pub fn point_from(lparam: isize) -> (i32, i32) {
    let bits = lparam as usize;
    let x = i32::from((bits & 0xFFFF) as u16 as i16);
    let y = i32::from(((bits >> 16) & 0xFFFF) as u16 as i16);
    (x, y)
}

// ---------------------------------------------------------------------------
// The PC's clock
// ---------------------------------------------------------------------------

/// Whether the user's Windows time format is a 12 or a 24 hour clock: an
/// `H` in it is a 24 hour hour, an `h` a 12 hour one, and text in single
/// quotes is left out. A format with neither is read as 12 hours.
pub fn clock_from_format(format: &str) -> Clock {
    let mut quoted = false;
    for c in format.chars() {
        match c {
            '\'' => quoted = !quoted,
            'H' if !quoted => return Clock::Hours24,
            'h' if !quoted => return Clock::Hours12,
            _ => {}
        }
    }
    Clock::Hours12
}

/// The PC's own clock: its offset from UTC now, and the user's time format.
pub fn pc_clock(utc_offset_secs: i32, time_format: &str) -> PcClock {
    PcClock::new(utc_offset_secs / 60, clock_from_format(time_format))
}

// ---------------------------------------------------------------------------
// Places on the screen
// ---------------------------------------------------------------------------

/// A rectangle on the screen, in pixels: a monitor, its work area, or a
/// window. The right and bottom edges are outside it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Area {
    /// The left edge.
    pub left: i32,
    /// The top edge.
    pub top: i32,
    /// The right edge, outside it.
    pub right: i32,
    /// The bottom edge, outside it.
    pub bottom: i32,
}

impl Area {
    /// Pixels across.
    pub fn width(&self) -> u32 {
        u32::try_from(i64::from(self.right) - i64::from(self.left)).unwrap_or(0)
    }

    /// Pixels down.
    pub fn height(&self) -> u32 {
        u32::try_from(i64::from(self.bottom) - i64::from(self.top)).unwrap_or(0)
    }

    /// Its size, as a picture for it is drawn: at least 1 by 1.
    pub fn size(&self) -> Size {
        Size::new(self.width().max(1), self.height().max(1))
    }

    /// True when the point is inside.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    /// Its middle.
    pub fn middle(&self) -> (i32, i32) {
        let x = (i64::from(self.left) + i64::from(self.right)) / 2;
        let y = (i64::from(self.top) + i64::from(self.bottom)) / 2;
        (to_i32(x), to_i32(y))
    }

    /// How far a point is from it, squared; 0 inside.
    fn distance_to(&self, x: i32, y: i32) -> i64 {
        let off = |v: i32, low: i32, high: i32| -> i64 {
            let (v, low, high) = (i64::from(v), i64::from(low), i64::from(high));
            if v < low {
                low - v
            } else if v >= high {
                v - high + 1
            } else {
                0
            }
        };
        let dx = off(x, self.left, self.right);
        let dy = off(y, self.top, self.bottom);
        dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
    }
}

fn to_i32(v: i64) -> i32 {
    i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(0)
}

fn side(v: u32) -> i64 {
    i64::from(v)
}

/// `v` kept between `low` and `high`; `low` when there is no room between.
fn keep(v: i64, low: i64, high: i64) -> i64 {
    if high < low { low } else { v.clamp(low, high) }
}

/// A design length at a scale, in whole pixels.
fn px(design: f32, scale: f32) -> i64 {
    let scale = if scale.is_finite() { scale.clamp(0.0, 10.0) } else { 1.0 };
    (design * scale).round() as i64
}

/// Design pixels between the time left display's card and the right edge of
/// the main monitor's work area, as the design places it.
pub const PILL_EDGE: f32 = 16.0;
/// Design pixels between the time left display's card and the top of the
/// taskbar: the design puts it 62 pixels above the bottom of a screen with a
/// 48 pixel taskbar.
pub const PILL_ABOVE: f32 = 14.0;
/// Design pixels between a notice's card and the time left display's card
/// under it: the design's 110, less the display's own 62 and its height of
/// 34.
pub const NOTICE_GAP: f32 = 14.0;
/// The time left display's hard shadow, down and to the right, in design
/// pixels (`box-shadow: 3px 3px 0` in the design). The design places the
/// card; its picture reaches past the card by its shadow.
pub const PILL_SHADOW: f32 = 3.0;
/// A notice's hard shadow, in design pixels (`box-shadow: 4px 4px 0`).
pub const NOTICE_SHADOW: f32 = 4.0;

/// How far a picture reaches past its card, right and down, in pixels: its
/// hard shadow at the scale, and the one pixel the pictures add to hold
/// their edge.
pub fn shadow_px(design: f32, scale: f32) -> i32 {
    to_i32(px(design, scale) + 1)
}

/// Where the time left display is: the bottom right corner of its card, on
/// the screen. Kept by the corner, so that as its words grow and shrink it
/// stays by the clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    /// The card's right edge.
    pub right: i32,
    /// The card's bottom edge.
    pub bottom: i32,
}

/// Where the time left display starts: above the taskbar's clock, its card
/// 16 design pixels in from the right edge of the main monitor's work area
/// and 14 above its bottom, at that monitor's scale, its shadow beyond.
pub fn start_anchor(work: Area, scale: f32) -> Anchor {
    Anchor {
        right: to_i32(i64::from(work.right) - px(PILL_EDGE, scale)),
        bottom: to_i32(i64::from(work.bottom) - px(PILL_ABOVE, scale)),
    }
}

/// The window for a picture of `size` whose card has its bottom right
/// corner at `anchor`, the picture reaching `shadow` pixels past its card.
pub fn rect_at(anchor: Anchor, size: Size, shadow: i32) -> Area {
    let right = i64::from(anchor.right) + i64::from(shadow);
    let bottom = i64::from(anchor.bottom) + i64::from(shadow);
    Area {
        left: to_i32(right - side(size.width)),
        top: to_i32(bottom - side(size.height)),
        right: to_i32(right),
        bottom: to_i32(bottom),
    }
}

/// The work area a window belongs on: the one its middle is in, or else the
/// nearest to its middle. None when there is none.
pub fn area_for(rect: Area, works: &[Area]) -> Option<Area> {
    let (x, y) = rect.middle();
    works.iter().find(|w| w.contains(x, y)).or_else(|| works.iter().min_by_key(|w| w.distance_to(x, y))).copied()
}

/// An anchor moved so that the whole window of `size`, its shadow too, lies
/// inside the work area it belongs on: wholly when it fits, from its top
/// left corner when it does not. Unchanged when there is no work area.
pub fn kept_on_screen(anchor: Anchor, size: Size, shadow: i32, works: &[Area]) -> Anchor {
    let rect = rect_at(anchor, size, shadow);
    let Some(work) = area_for(rect, works) else { return anchor };
    let (w, h) = (side(size.width), side(size.height));
    let right = keep(i64::from(rect.right), i64::from(work.left) + w, i64::from(work.right));
    let bottom = keep(i64::from(rect.bottom), i64::from(work.top) + h, i64::from(work.bottom));
    Anchor { right: to_i32(right - i64::from(shadow)), bottom: to_i32(bottom - i64::from(shadow)) }
}

/// Where a drag puts the time left display: moved by as much as the pointer
/// moved since it was pressed, then kept on the screen.
pub fn dragged(from: Anchor, pressed: (i32, i32), now: (i32, i32), size: Size, shadow: i32, works: &[Area]) -> Anchor {
    let dx = i64::from(now.0) - i64::from(pressed.0);
    let dy = i64::from(now.1) - i64::from(pressed.1);
    let moved = Anchor { right: to_i32(i64::from(from.right) + dx), bottom: to_i32(i64::from(from.bottom) + dy) };
    kept_on_screen(moved, size, shadow, works)
}

/// Where a notice's picture of `size` goes, as its top left corner, by the
/// time left display's window `pill`: the notice's card above the display's
/// card, their right edges in line, with the design's gap between; below it
/// when the work area has no room above; and kept inside the work area.
/// Each picture reaches past its card by its shadow, `pill_shadow` and
/// `shadow` pixels.
pub fn notice_at(pill: Area, pill_shadow: i32, size: Size, shadow: i32, work: Area, scale: f32) -> (i32, i32) {
    let gap = px(NOTICE_GAP, scale);
    let (w, h, shadow) = (side(size.width), side(size.height), i64::from(shadow));
    let card_right = i64::from(pill.right) - i64::from(pill_shadow);
    let card_bottom = i64::from(pill.bottom) - i64::from(pill_shadow);
    let above = i64::from(pill.top) - gap - (h - shadow);
    let top = if above >= i64::from(work.top) { above } else { card_bottom + gap };
    let left = keep(card_right + shadow - w, i64::from(work.left), i64::from(work.right) - w);
    let top = keep(top, i64::from(work.top), i64::from(work.bottom) - h);
    (to_i32(left), to_i32(top))
}

// ---------------------------------------------------------------------------
// Pixels
// ---------------------------------------------------------------------------

/// Copies a picture's pixels (RGBA, premultiplied, rows from the top) into
/// a Windows bitmap's (BGRA, premultiplied, rows from the top), as much of
/// the picture as the bitmap holds. True when any pixel of the bitmap
/// changed, so that only a picture that changed is put on the screen again.
/// Buffers of the wrong length change nothing.
pub fn copy_to_bgra(picture: &[u8], size: Size, bitmap: &mut [u8], width: u32, height: u32) -> bool {
    let bytes = |w: u32, h: u32| (w as usize).checked_mul(h as usize).and_then(|n| n.checked_mul(4));
    if bytes(size.width, size.height) != Some(picture.len()) || bytes(width, height) != Some(bitmap.len()) {
        return false;
    }
    let (pw, bw) = (size.width as usize * 4, width as usize * 4);
    let across = pw.min(bw);
    let down = size.height.min(height) as usize;
    let mut row = vec![0u8; across];
    let mut changed = false;
    for y in 0..down {
        let (Some(from), Some(to)) = (picture.get(y * pw..y * pw + across), bitmap.get_mut(y * bw..y * bw + across)) else {
            break;
        };
        for (out, px) in row.as_chunks_mut::<4>().0.iter_mut().zip(from.as_chunks::<4>().0) {
            *out = [px[2], px[1], px[0], px[3]];
        }
        if to != row.as_slice() {
            to.copy_from_slice(&row);
            changed = true;
        }
    }
    changed
}

// ---------------------------------------------------------------------------
// Notices
// ---------------------------------------------------------------------------

/// How long a warning stays, in milliseconds, unless OK puts it away first:
/// as long as the plain display's.
pub const WARNING_SHOWN_MS: u64 = 30_000;
/// How long a notice of a program closed stays, in milliseconds.
pub const NOTICE_SHOWN_MS: u64 = 10_000;
/// The most notices kept waiting their turn.
pub const MAX_SHELVED: usize = 4;

/// A notice waiting its turn above the time left display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shelved {
    /// A lock is coming.
    Warning(WarningShown),
    /// A program was closed.
    Closed {
        /// The service's sentence.
        text: String,
        /// The program's name, when the service gave it.
        program: Option<String>,
    },
}

/// The notices: the newest is shown, and when OK or its time puts it away,
/// the one before it, if its own time has not run out.
#[derive(Debug, Default)]
pub struct Shelf {
    items: Vec<(Shelved, u64)>,
    warnings_seen: Vec<(WarningCause, u32, i64)>,
}

impl Shelf {
    /// A warning from a show. Each warning is shelved once, the first time
    /// it comes: the service sends it again with every show until it hears
    /// it was shown.
    pub fn warn(&mut self, warning: &WarningShown, now_ms: u64) {
        let key = (warning.cause, warning.minutes, warning.lock_at);
        if self.warnings_seen.contains(&key) {
            return;
        }
        if self.warnings_seen.len() >= 16 {
            self.warnings_seen.remove(0);
        }
        self.warnings_seen.push(key);
        self.put(Shelved::Warning(warning.clone()), now_ms.saturating_add(WARNING_SHOWN_MS));
    }

    /// A notice that a program was closed.
    pub fn closed(&mut self, text: &str, program: Option<&str>, now_ms: u64) {
        let program = program.map(str::trim).filter(|p| !p.is_empty()).map(str::to_string);
        self.put(Shelved::Closed { text: text.to_string(), program }, now_ms.saturating_add(NOTICE_SHOWN_MS));
    }

    fn put(&mut self, what: Shelved, until_ms: u64) {
        self.items.retain(|(kept, _)| *kept != what);
        if self.items.len() >= MAX_SHELVED {
            self.items.remove(0);
        }
        self.items.push((what, until_ms));
    }

    /// The notice to show now: the newest whose time has not run out.
    pub fn top(&mut self, now_ms: u64) -> Option<&Shelved> {
        self.items.retain(|(_, until)| now_ms < *until);
        self.items.last().map(|(what, _)| what)
    }

    /// OK was clicked: the notice shown goes.
    pub fn dismiss(&mut self) {
        self.items.pop();
    }
}

// ---------------------------------------------------------------------------
// The display as a machine
// ---------------------------------------------------------------------------

/// What the drawn display shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Nothing of LiMo.
    Nothing,
    /// The time left display, and its notices above it.
    TimeLeft,
    /// The lock on every monitor.
    Locked,
}

/// A moment, both ways: the wall clock for the words, unix seconds, and the
/// steady count for how long a notice stays, in milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Moment {
    /// Unix seconds.
    pub wall: i64,
    /// A steady count of milliseconds.
    pub steady_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LeftNow {
    seconds_left: Option<u64>,
    counting: bool,
    unlocked_until: Option<i64>,
}

/// The drawn display as a machine, with no windows: the lock screen and the
/// time left display, what the service last ordered, the notices, the way
/// out waiting for its answer, and the modifier keys held.
///
/// - A way out the lock gives goes to the [`Outbox`] once; the number the
///   outbox gives it is kept, and only the answer that carries that number
///   reaches the lock, and only while the same lock is up. A lock taken
///   down forgets it.
/// - A typed PIN or code moves out of the lock's wiped store into the
///   protocol's [`Secret`] without a copy: the lock's own buffer is taken,
///   not copied, and the secret wipes it when it is dropped after it is
///   sent.
#[derive(Debug)]
pub struct DrawnCore {
    outbox: Outbox,
    lock: Lock,
    overlay: Overlay,
    mode: Mode,
    pc: PcClock,
    at: Moment,
    waiting: Option<u32>,
    left: Option<LeftNow>,
    shelf: Shelf,
    mods: Mods,
}

impl DrawnCore {
    /// A display showing nothing, sending its ways out to `outbox`.
    pub fn new(outbox: Outbox) -> DrawnCore {
        DrawnCore {
            outbox,
            lock: Lock::new(),
            overlay: Overlay::new(),
            mode: Mode::Nothing,
            pc: PcClock::default(),
            at: Moment::default(),
            waiting: None,
            left: None,
            shelf: Shelf::default(),
            mods: Mods::default(),
        }
    }

    /// The PC's clock now, read again by the Windows side from time to time.
    pub fn set_clock(&mut self, pc: PcClock) {
        self.pc = pc;
    }

    /// What it shows now.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The number of the way out waiting for its answer, if one is.
    pub fn waiting(&self) -> Option<u32> {
        self.waiting
    }

    /// The lock screen, to ask what is under a point or how it stands.
    pub fn lock(&mut self) -> &mut Lock {
        &mut self.lock
    }

    /// What the service orders now, as a level. Says what it shows.
    pub fn show(&mut self, now: &ShowNow<'_>, at: Moment) -> Mode {
        self.at = at;
        match now.show {
            Show::Nothing => {
                self.leave_lock();
                self.left = None;
                self.mode = Mode::Nothing;
            }
            Show::TimeLeft { seconds_left, counting, warning, unlocked_until } => {
                self.leave_lock();
                self.left = Some(LeftNow { seconds_left: *seconds_left, counting: *counting, unlocked_until: *unlocked_until });
                if let Some(warning) = warning {
                    self.shelf.warn(warning, at.steady_ms);
                }
                self.mode = Mode::TimeLeft;
            }
            Show::Locked { screen } => {
                if self.mode != Mode::Locked {
                    self.fresh_lock();
                }
                self.lock.show(screen, now.child_name, at.wall, self.pc);
                self.mode = Mode::Locked;
            }
        }
        self.mode
    }

    /// The lock starts afresh: nothing typed, no way out waiting, no key
    /// held.
    fn fresh_lock(&mut self) {
        self.lock.reset();
        self.waiting = None;
        self.mods = Mods::default();
    }

    fn leave_lock(&mut self) {
        if self.mode == Mode::Locked {
            self.fresh_lock();
        }
    }

    /// A program was closed.
    pub fn notice(&mut self, text: &str, program: Option<&str>, at: Moment) {
        self.at = at;
        self.shelf.closed(text, program, at.steady_ms);
    }

    /// Time passed.
    pub fn tick(&mut self, at: Moment) {
        self.at = at;
        if self.mode == Mode::Locked {
            self.take(Input::Time { now: at.wall });
        }
    }

    /// True when a key pressed now should be turned into text with the
    /// user's own keyboard layout: the ask panel is open, and no shortcut
    /// key is held.
    pub fn text_wanted(&self) -> bool {
        self.mode == Mode::Locked && self.lock.panel() == Panel::Ask && self.mods.types_text()
    }

    /// The keyboard state for turning a key into text now.
    pub fn keyboard_state(&self, caps_lock: bool) -> [u8; 256] {
        self.mods.keyboard_state(caps_lock)
    }

    /// A key went down or up, as the keyboard hook saw it, with the text the
    /// user's layout gives for it when [`DrawnCore::text_wanted`] said so.
    /// Only a lock takes keys.
    pub fn key(&mut self, vk: u32, down: bool, text: Option<char>) {
        self.mods.note(vk, down);
        if !down || self.mode != Mode::Locked {
            return;
        }
        let wanted = self.lock.panel() == Panel::Ask;
        let text = text.filter(|_| self.mods.types_text());
        if let Some(key) = lock_key(vk, text, wanted) {
            self.take(Input::Key(key));
        }
    }

    /// A click on the main monitor's lock, in its picture's pixels.
    pub fn click(&mut self, x: f32, y: f32) {
        if self.mode == Mode::Locked {
            self.take(Input::Click { x, y });
        }
    }

    /// The answer to a way out. It reaches the lock only when it carries the
    /// number of the way out this lock is waiting for. True when it did.
    pub fn answer(&mut self, ask: u32, answer: &Answer) -> bool {
        if self.mode != Mode::Locked || self.waiting != Some(ask) {
            return false;
        }
        self.waiting = None;
        self.lock.input(Input::Answer(lock_answer(answer)));
        true
    }

    /// Hands something to the lock, and sends the way out it gives.
    fn take(&mut self, input: Input) {
        if let Some(way) = self.lock.input(input) {
            self.send(way);
        }
        // The lock stops waiting by itself after a while; so does this.
        if self.lock.waiting_for().is_none() {
            self.waiting = None;
        }
    }

    /// Sends a way out once, through the outbox. A typed PIN or code is
    /// moved, not copied, into the secret that carries it.
    fn send(&mut self, way: WayOut) {
        let ask = match way {
            WayOut::AskForTime { minutes, reason } => self.outbox.ask_for_time(minutes, &reason),
            WayOut::TryCode { code } => self.outbox.try_code(Secret::new(code.take())),
            WayOut::TryPin { pin, action } => self.outbox.try_pin(Secret::new(pin.take()), action),
        };
        self.waiting = Some(ask);
    }

    /// The main monitor's lock at this size, in pixels.
    pub fn lock_picture(&mut self, size: Size) -> &Picture {
        self.lock.draw(size)
    }

    /// Every other monitor's lock at this size.
    pub fn other_picture(&mut self, size: Size) -> &Picture {
        self.lock.draw_other(size)
    }

    /// The time left display at a monitor's scale, while it is shown.
    pub fn pill(&mut self, scale: f32) -> Option<&Picture> {
        if self.mode != Mode::TimeLeft {
            return None;
        }
        let left = self.left?;
        let shown = TimeLeft {
            seconds_left: left.seconds_left,
            counting: left.counting,
            unlocked_until: left.unlocked_until,
            now: self.at.wall,
            pc: self.pc,
        };
        Some(self.overlay.time_left(&shown, scale))
    }

    /// The notice to show above the time left display at a monitor's scale,
    /// if there is one, with where its OK button is in its pixels.
    pub fn notice_picture(&mut self, scale: f32) -> Option<(&Picture, PixelRect)> {
        if self.mode != Mode::TimeLeft {
            return None;
        }
        let notice = match self.shelf.top(self.at.steady_ms)? {
            Shelved::Warning(warning) => Notice::Warning { warning: warning.clone(), now: self.at.wall, pc: self.pc },
            Shelved::Closed { program: Some(program), .. } => Notice::Blocked { program: program.clone() },
            Shelved::Closed { text, program: None } => Notice::Closed { text: text.clone() },
        };
        self.overlay.notice(&notice, scale);
        let ok = self.overlay.notice_ok()?;
        Some((self.overlay.notice(&notice, scale), ok))
    }

    /// A click on the notice, in its picture's pixels: on its OK button, the
    /// notice goes. True when it went.
    pub fn notice_click(&mut self, ok: PixelRect, x: f32, y: f32) -> bool {
        if self.mode == Mode::TimeLeft && ok.contains(x, y) {
            self.shelf.dismiss();
            return true;
        }
        false
    }
}

/// The service's answer to a way out, as the lock screen takes it: the same
/// three cases.
pub fn lock_answer(answer: &Answer) -> LockAnswer {
    match answer {
        Answer::Asked { request } => LockAnswer::Asked { request: request.clone() },
        Answer::Tried { outcome } => LockAnswer::Tried { outcome: outcome.clone() },
        Answer::Refused { message } => LockAnswer::Refused { message: message.clone() },
    }
}

#[cfg(test)]
mod tests;
