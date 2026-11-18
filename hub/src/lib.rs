//! # LiMo hub store
//!
//! The family's data on the master PC, in one SQLite file: the guardian
//! password, the children, their rules, their devices, the day's pools, the
//! usage, the Windows accounts and a short record of events. The Windows
//! service holds one [`Hub`] and calls it; the hub calls the rules core
//! ([`limo_core`]) for every rule, pool and grant.
//!
//! The hub has no network, no Windows code, no user interface, no threads and
//! no clock. Every call that needs the time is given it, in unix seconds by
//! the master PC's clock, exactly as the core is. It takes random bytes (ids,
//! salts, code secrets) from the operating system.
//!
//! The promises, in short (the README next to this crate says more):
//!
//! - **Saved before sent.** [`Hub::handle_report`] returns a grant only after
//!   the book that made it has been written to the file and the write has
//!   reached the disk.
//! - **All or nothing.** Every call that changes something does it in one
//!   transaction. A failure, or a crash, part way through leaves the file as
//!   it was before the call.
//! - **Deleted means deleted.** Removing a child, or deleting the family,
//!   removes the rows and overwrites the space they took in the file.
//! - **One hub per file.** While a hub has the file open, a second one is
//!   refused with [`HubError::InUse`].
//! - **Secrets stay out of sight.** The password, the PIN and the code
//!   secrets never appear in `Debug` output or in error messages. They are in
//!   the file, in the forms the core defines; the file is protected by the
//!   Windows permissions the installer sets, not by this crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod accounts;
mod children;
mod devices;
mod error;
mod events;
mod family;
mod guardian;
mod hub;
mod password;
mod random;
mod retire;
mod schema;
mod store;
mod text;
mod tidy;
mod types;
mod views;

pub use error::HubError;
pub use hub::Hub;
pub use password::{MAX_PASSWORD_CHARS, MIN_PASSWORD_CHARS, PASSWORD_ROUNDS};
pub use schema::SCHEMA_VERSION;
pub use text::TextField;
pub use types::{
    Account, AccountLink, AccountStanding, AgeBand, AppUsage, Child, ChildToday, DeviceInfo, DeviceToday,
    DeviceUsage, Event, EventKind, ExtraGiven, Pairing, PasswordOutcome, TidyReport, UsageView, WaitingRequest,
};

/// How many days of usage are kept slice by slice when the guardian has not
/// chosen: 35. Older days are folded into daily totals by [`Hub::tidy`].
pub const DEFAULT_DETAIL_DAYS: u32 = 35;
/// The fewest days of usage that can be kept slice by slice: 7.
pub const MIN_DETAIL_DAYS: u32 = 7;
/// The most days of usage that can be kept slice by slice: 400.
pub const MAX_DETAIL_DAYS: u32 = 400;

/// How many home days each child's book in the core keeps: today and
/// yesterday. The book is written on every exchange, so it is kept small; the
/// usage the hub shows lives in its own tables. See the README.
pub const BOOK_DAYS: i64 = 2;

/// A device counts as in use when the master heard from it within this many
/// seconds (10 minutes) ...
pub const IN_USE_HEARD_SECS: i64 = 600;
/// ... and that report carried use that ended at most this many seconds (2
/// minutes) before the report was sent, by the device's own clock.
pub const IN_USE_FRESH_SECS: i64 = 120;

/// A device's clock is far off when a report's sending time differs from the
/// master's clock by more than this many seconds: 5 minutes.
pub const CLOCK_FAR_OFF_SECS: i64 = 300;

/// Slices for a home day more than this many days from the master's today are
/// counted by the core but not kept in the hub's usage record: such a day
/// comes only from a clock that is badly wrong.
pub const FAR_DAY_DAYS: i64 = 8;

/// Events older than this many days are dropped by [`Hub::tidy`]: 90.
pub const EVENT_KEEP_DAYS: i64 = 90;
/// At most this many events are kept, the newest: 2,000.
pub const MAX_EVENTS: u32 = 2_000;

/// The longest range of days [`Hub::usage`] answers for at once: 400.
pub const MAX_USAGE_DAYS: u32 = 400;

/// The most events [`Hub::events`] returns at once: 500.
pub const MAX_EVENTS_PER_CALL: u32 = 500;

/// The shortest guardian PIN, in digits: 6, by the owner's decision. The
/// hub refuses a shorter PIN itself, before the core sees it.
pub const MIN_PIN_DIGITS: u32 = 6;
/// The longest guardian PIN, in digits: the core's limit, 8.
pub const MAX_PIN_DIGITS: u32 = limo_core::pin::MAX_PIN_DIGITS;

/// The version of this crate, for example "0.1.0".
pub fn hub_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
