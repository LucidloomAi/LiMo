//! # LiMo agent engine
//!
//! The engine of the agent on the master PC: the part of the Windows service
//! that decides, second by second, what happens in each Windows session. It
//! holds no Windows code. Everything the PC knows comes in as plain facts
//! ([`Facts`]), and everything to be done goes out as plain orders
//! ([`Orders`]). The service calls [`Engine::tick`] about once a second and
//! carries out what comes back; a lock screen's ways out go through
//! [`Engine::ask_for_time`], [`Engine::try_code`] and [`Engine::try_pin`].
//!
//! On the master PC each child has one device of kind "on the master": the
//! PC itself, as that child uses it. The engine is the first real caller of
//! the device side of the rules core ([`limo_core::DeviceState`]); it talks
//! to the child's book in the family's file ([`limo_hub::Hub`]) through the
//! same report and grant a phone sends, in process, and keeps the device's
//! state in the same file, written in the same transactions as the book.
//!
//! What it promises, in short (the README next to this crate says more):
//!
//! - **Which program and for how long, and nothing else.** A program is its
//!   file name, the original name its maker recorded inside it, and its
//!   maker. No type in this crate has a field for a window title, a
//!   document, a web address or anything typed.
//! - **Durations come from the steady count.** The wall clock says what time
//!   it is, never how long anything lasted. A clock set back or forward, a
//!   time zone changed, a PC asleep or a service held up never adds or
//!   removes counted time.
//! - **A restart at any instant is safe.** Nothing is counted twice, no time
//!   handed out is handed out again, and a lock that was on is on again at
//!   the first call.
//!
//! It has no clock, no threads, no network and no files of its own, and no
//! unsafe code.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod child;
mod engine;
mod error;
mod facts;
mod orders;

pub use engine::{ChildOnPc, Engine};
pub use error::{EngineError, HubStep};
pub use facts::{Facts, Program, Session, SessionState};
pub use orders::{CloseOrder, LockScreen, LockWhy, Orders, SessionOrder, Show, WarningShown};

/// While a child's time is being counted, use is reported to the child's
/// book at least this often, in seconds: once a minute.
pub const REPORT_EVERY_SECS: u64 = 60;

/// A step between two calls longer than this, by the steady count, counts
/// nothing at all, in milliseconds: 10 seconds. The PC slept, or the service
/// was held up, and what happened meanwhile is not known.
pub const MAX_COUNTED_STEP_MS: u64 = 10_000;

/// The wall clock and the steady count may drift apart by this much between
/// two calls before the engine takes it as the clock being set, in
/// milliseconds: 5 seconds.
pub const CLOCK_JUMP_MS: u64 = 5_000;

/// The version of this crate, for example "0.1.0".
pub fn agent_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
