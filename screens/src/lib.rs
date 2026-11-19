//! # LiMo's screens on the PC
//!
//! What a child sees on the master PC, drawn into pixels in software: the
//! lock screen ([`Lock`]), the time left display and its notices
//! ([`Overlay`]). The look is the one agreed in the repository's `design/`
//! folder (the Paper and Ink theme, `design/screens/pc.css` and `pc.js`),
//! and these pictures follow it: the same layout, sizes, colours, borders,
//! hard shadows, dashed dividers, stamp, icon tiles, type and words.
//!
//! There is no operating system code here. A helper (the Windows one today,
//! perhaps a Mac one later) puts the pictures on the screen and hands in
//! clicks and keys; this crate decides nothing about the rules, it only
//! shows what the engine orders and passes on what the child chooses.
//!
//! * **The lock screen** is a small machine: it is told what to show
//!   ([`Lock::show`]), takes clicks, keys, the passing of time and the
//!   answers to its ways out ([`Lock::input`]), gives back the way out to
//!   send ([`WayOut`]), and draws itself at any size ([`Lock::draw`],
//!   [`Lock::draw_other`] for every other monitor). What is under a point
//!   comes from the same layout ([`Lock::target_at`]).
//! * **The time left display and the notices** are small pictures of their
//!   own size ([`Overlay::time_left`], [`Overlay::notice`]).
//!
//! Everything is laid out for a screen of 1366 by 768 and scaled as one
//! piece by [`scale_for`]. The three fonts, the icons and the logo are
//! built in; nothing is read from disk, there is no clock and no thread,
//! and no unsafe code.
//!
//! A typed PIN or code is kept in memory that is wiped when it is sent or
//! the panel is left; it never appears in `Debug` output, and is drawn only
//! as dots (the PIN) or as digits in their cells (the code).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod icons;
mod lock;
mod overlay;
mod paint;
mod picture;
pub mod samples;
mod scene;
mod text;
mod theme;
pub mod words;

pub use lock::{
    ANSWER_WAIT_SECS, ASK_CHOICES, Answer, CODE_DIGITS, Input, Key, Lock, OPENED_SECS, Panel, PinChoice, Target, Typed,
    Way, WayOut, lock_scale,
};
pub use overlay::{Notice, Overlay, PillState, PixelRect, TimeLeft};
pub use paint::Painter;
pub use picture::{MAX_SIDE, Picture, Size};
pub use theme::{DESIGN_HEIGHT, DESIGN_WIDTH, MAX_SCALE, MIN_SCALE, scale_for};
pub use words::{Clock, PcClock};

/// The version of this crate, for example "0.1.0".
pub fn screens_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
