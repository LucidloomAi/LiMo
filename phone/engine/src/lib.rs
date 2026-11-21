//! # LiMo phone engine
//!
//! The engine of LiMo on a child's Android phone: the part that decides,
//! second by second, what the phone shows. It holds no Android code.
//! Everything the phone knows comes in as plain facts ([`PhoneFacts`]), and
//! everything to be done goes out as plain orders ([`PhoneOrders`]). The
//! host (the Android service) calls [`PhoneEngine::tick`] about once a
//! second and carries out what comes back; it carries the reports and the
//! grants between [`PhoneEngine::outgoing_report`] and the master; a lock's
//! ways out go through [`PhoneEngine::ask_for_time`],
//! [`PhoneEngine::try_code`] and [`PhoneEngine::try_pin`]; and it writes the
//! bytes [`PhoneEngine::save`] gives it, and hands them back to
//! [`PhoneEngine::load`] after a restart.
//!
//! The phone is one device of one child, in the rules core's sense
//! ([`limo_core::DeviceState`]): it shares the child's one daily budget with
//! the master PC and the child's other devices through the same report and
//! grant the PC's own devices use, and works through a whole day out of
//! reach on its away allowance.
//!
//! What it promises, in short (the README next to this crate says more):
//!
//! - **Which app and for how long, and nothing else.** An app is its package
//!   name. No type in this crate has a field for a window's text, a web
//!   address, a message, a contact or anything typed.
//! - **A lock never stands between a child and a call for help.** Nothing is
//!   covered while a call rings, nor ever the dialler, the emergency dialler
//!   or LiMo itself. **But a call is no way around the limits:** during a
//!   call in progress any other app counts and is covered as with no call,
//!   and the cover offers the way back to the call.
//! - **Durations come from the elapsed count.** The wall clock says what
//!   time it is, never how long anything lasted, and while automatic time
//!   is off a wall clock set by hand is not followed: it buys nothing.
//! - **A restart at any instant is safe**, as long as the host keeps the
//!   state when the engine says so: nothing is counted twice, no time handed
//!   out is handed out again, and a lock that was on is on again at the
//!   first call.
//! - **Setup is the guardian's time, not the child's.** From pairing until a
//!   guardian finishes setup on the phone ([`PhoneEngine::finish_setup`]),
//!   nothing is counted or covered, and the time left says so.
//! - **What could switch LiMo off is guarded.** A page the host recognises
//!   as one ([`GuardedPage`]) is covered by the guard, which offers the
//!   guardian PIN only; a right PIN opens such pages for a few minutes
//!   ([`PhoneEngine::try_guard_pin`]).
//!
//! It has no clock, no threads, no network and no files of its own, and no
//! unsafe code.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod counts;
mod engine;
mod error;
mod facts;
mod kept;
mod orders;
mod today;
mod words;

pub use counts::{app_in_use, may_cover};
pub use engine::{PhoneEngine, PhoneView, SetupView};
pub use error::{LoadProblem, PhoneError};
pub use facts::{CallState, Front, GuardedPage, Permission, PhoneFacts, PhonePermissions};
pub use words::MAX_LABEL_CHARS;
pub use orders::{
    Cover, GrantTaken, GuardScreen, LockScreen, LockWhy, PhoneNote, PhoneOrders, RequestLine, TimeLeft, TryAnswer,
    WarningShown, Way, WayRow,
};
pub use today::{GuardianNeeded, LeftKind, RulesWords, SwitchedOff, TodayApp, TodayLeft, TodayView};

/// While there is use the master has not had, and the master is in reach,
/// a report goes at least this often, in seconds: once a minute.
pub const REPORT_EVERY_SECS: u64 = 60;

/// While the master is in reach, a report goes at least this often even
/// with nothing to report, in seconds: every five minutes, so that new
/// rules reach the phone on a day with no limit.
pub const REPORT_IDLE_SECS: u64 = 300;

/// A report not answered within this long, in milliseconds (10 seconds), has
/// failed: the host calls [`PhoneEngine::lost_contact`], and no other report
/// is asked for meanwhile.
pub const ANSWER_WAIT_MS: u64 = 10_000;

/// A step between two calls longer than this, by the elapsed count, counts
/// nothing at all, in milliseconds: 10 seconds. LiMo was held up or
/// stopped, and what happened meanwhile is not known.
pub const MAX_COUNTED_STEP_MS: u64 = 10_000;

/// A step this long or longer while the phone was in use is noted for the
/// guardian ([`PhoneNote::NotWatched`]), in milliseconds: a minute.
pub const NOT_WATCHED_NOTE_MS: u64 = 60_000;

/// The wall clock and the elapsed count may drift apart by this much between
/// two calls before the engine takes it as the clock being set, in
/// milliseconds: 5 seconds.
pub const CLOCK_JUMP_MS: u64 = 5_000;

/// While counting just goes on, the state is kept at most this often, in
/// milliseconds: every five seconds. Anything else that changes is kept at
/// once.
pub const SAVE_PROGRESS_EVERY_MS: u64 = 5_000;

/// The notes for the guardian kept until the master takes them: the newest
/// 200. No more than the master takes from one report.
pub const MAX_NOTES_KEPT: usize = 200;
const _: () = assert!(MAX_NOTES_KEPT <= limo_core::MAX_NOTES_PER_REPORT);

/// The most apps whose use today the engine keeps for the child's Today
/// screen: 200. A new app beyond them takes the place of the one with the
/// least time.
pub const MAX_APPS_A_DAY: usize = 200;

/// The form of the kept state this engine writes, and the newest it reads.
pub const STATE_FORMAT: u32 = 1;

/// How long a right guardian PIN on the guard keeps the guarded pages open,
/// in seconds: five minutes, by the elapsed count. Long enough for a
/// guardian to switch the protection off and remove LiMo, or to change an
/// accessibility setting for the child, across a few pages; short enough
/// that a phone handed back still on is guarded again within minutes. It
/// closes at once when the screen goes off or the phone starts again
/// ([`PhoneEngine::try_guard_pin`]).
pub const GUARD_OPEN_SECS: u64 = 300;

/// The version of this crate, for example "0.1.0".
pub fn phone_engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
