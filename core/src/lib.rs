//! # LiMo rules core
//!
//! The one place where LiMo's rules and arithmetic live. The Windows service
//! uses it directly; the Android app will use it through generated Kotlin
//! bindings. It answers two questions:
//!
//! 1. **May this child use this device now, for how long, and if not, why
//!    not and until when?** See [`decide::decide`] and
//!    [`device::DeviceState::decide`].
//! 2. **How does one daily budget stay honest across a PC and a phone with no
//!    server?** See [`master::MasterBook`] (the master's side) and
//!    [`device::DeviceState`] (each device's side), and the messages between
//!    them in [`messages`].
//!
//! It also holds the ways out, so that a child is never stuck and a
//! guardian can act with no server: extra time and requests for it
//! ([`extra`]), one time codes ([`codes`]), the guardian PIN ([`pin`]), and a
//! guardian's lock or unlock ([`guardian`]). A device's notes for the
//! guardian ride on its reports ([`notes`]).
//!
//! The library is pure: no clock, no files, no network, no threads, no
//! randomness and nothing global. Every function that needs the time is
//! given it, in unix seconds. Days and weekdays are always those of the home
//! time zone, which travels with the rules ([`time`]).
//!
//! The README next to this crate explains the model in plain words, with a
//! worked day, and states exactly what the shared budget guarantees.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod blocked;
pub mod codes;
pub mod decide;
pub mod device;
pub mod extra;
pub mod guardian;
pub mod master;
pub mod messages;
pub mod notes;
pub mod pin;
pub mod rules;
pub mod time;

pub use blocked::{BlockedHours, LOOK_AHEAD_SECS};
pub use codes::{
    CODE_MINUTES, CODE_SECRET_BYTES, CODES_PER_DAY, CodeError, OneTimeCode, code_digits, make_code,
    next_code,
};
pub use decide::{
    Decision, Holding, Lock, LockReason, MasterWord, RETRY_SECS, Warned, Warning, WarningCause,
    decide,
};
pub use device::{DeviceError, DeviceState, GrantOutcome, OthersUsed, TryOutcome};
pub use extra::{
    AddedBy, AddedTime, ExtraSource, ExtraTime, MAX_ASK_MINUTES, MAX_EXTRA_MINUTES, MAX_REASON_CHARS,
    MAX_REQUESTS_PER_DAY, RequestAnswer, RequestReply, RequestView, TimeRequest,
};
pub use guardian::{
    FREE_WRONG_GUESSES, GuardianMode, GuardianSetting, MAX_UNLOCK_SECS, OverrideBy, OverrideInForce,
    OverrideKind, PinUnlock, Standing, TryCounter, wait_after_wrong_guesses,
};
pub use master::{
    CountedUse, DeviceKind, DeviceView, MasterBook, MasterError, NEED_KEPT_SECS, PoolView, ReportOutcome,
};
pub use messages::{Grant, Report, Slice};
pub use notes::{GuardedPage, MAX_NOTES_PER_REPORT, PhoneNote, PhonePermission, ProtectionLevel, SentNote};
pub use pin::{
    MAX_PIN_DIGITS, MIN_PIN_DIGITS, MIN_TYPED_PIN_DIGITS, PIN_ITERATIONS, PinAction, PinError, PinRecord,
    PinUpdate, pin_is_well_formed, typed_pin_is_well_formed,
};
pub use rules::{BlockedWindow, DailyBudget, RuleProblem, RuleSet, WeekBudgets};
pub use time::{CivilDate, DaySpan, HomeCalendar, LocalTime, SECONDS_PER_DAY, Weekday};

/// The version of this crate, for example "0.1.0".
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
