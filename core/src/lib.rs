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
pub mod decide;
pub mod device;
pub mod master;
pub mod messages;
pub mod rules;
pub mod time;

pub use blocked::{BlockedHours, LOOK_AHEAD_SECS};
pub use decide::{
    Decision, Holding, Lock, LockReason, MasterWord, RETRY_SECS, Warned, Warning, WarningCause,
    decide,
};
pub use device::{DeviceState, GrantOutcome};
pub use master::{DeviceKind, DeviceView, MasterBook, MasterError, NEED_KEPT_SECS, PoolView};
pub use messages::{Grant, Report, Slice};
pub use rules::{BlockedWindow, DailyBudget, RuleProblem, RuleSet, WeekBudgets};
pub use time::{CivilDate, DaySpan, HomeCalendar, LocalTime, SECONDS_PER_DAY, Weekday};

/// The version of this crate, for example "0.1.0".
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
