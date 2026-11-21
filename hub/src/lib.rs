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
//! - **The master PC's own devices.** Each child's device on the master PC
//!   is paired once ([`Hub::pair_master_pc`]), and its state is kept in the
//!   same file and written in the same transaction as the child's book
//!   ([`Hub::exchange_on_master_pc`]), for the agent on the master PC.
//! - **The link's records.** The master's own key, made with the family
//!   ([`Hub::master_key`]); the pairing offers ([`Hub::keep_offer`]); a phone
//!   paired, its key kept and the offer marked used in one transaction
//!   ([`Hub::pair_device_with_offer`]); and each phone's notes, kept with
//!   the report that carries them ([`Hub::handle_report`]), and its apps
//!   ([`Hub::keep_phone_apps`]).

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
mod link;
mod master_pc;
mod notes;
mod password;
mod phones;
mod random;
mod schema;
mod store;
mod text;
mod tidy;
mod types;
mod views;

pub use error::HubError;
pub use hub::Hub;
pub use notes::{GuardedPage, MAX_NOTES_PER_REPORT, PhoneNote, PhonePermission, ProtectionLevel};
pub use password::{MAX_PASSWORD_CHARS, MIN_PASSWORD_CHARS, PASSWORD_ROUNDS};
pub use schema::SCHEMA_VERSION;
pub use text::TextField;
pub use types::{
    Account, AccountLink, AccountStanding, AgeBand, AppUsage, Child, ChildToday, DeviceInfo, DeviceToday,
    DeviceUsage, Event, EventKind, ExtraGiven, LinkPhone, MasterKey, MasterPcExchange, MasterPcRecord, OfferRecord,
    Pairing, PasswordOutcome, PcSettings, PhoneApp, PhoneApps, PhoneForm, PhoneInfo, PhoneRequest, PinOutcome, TidyReport,
    UnlinkedAccounts, UsageView, WaitingRequest,
};

/// The length of the master's private key for the link, and of a phone's
/// link key, in bytes: an X25519 key.
pub const KEY_BYTES: usize = 32;

/// The length of a pairing offer's id, in hex digits: 8 bytes.
pub const OFFER_ID_HEX_DIGITS: usize = 16;
/// The longest text form of a pairing offer the hub keeps, in characters.
/// The link's is about 150 with two addresses, and at most about 190.
pub const MAX_OFFER_TEXT_CHARS: usize = 512;
/// The longest an offer may last, in seconds: a day. The link's last ten
/// minutes.
pub const MAX_OFFER_LIFE_SECS: i64 = 86_400;
/// How long an offer is kept after it expires, in seconds: a day. Until
/// then, a phone that comes late with it can be told that it expired or was
/// used; then [`Hub::tidy`] drops it.
pub const OFFER_KEEP_SECS: i64 = 86_400;

/// The most apps a phone's list may hold: 2,000, as the link allows.
pub const MAX_PHONE_APPS: usize = 2_000;
/// The longest package name, in bytes: 255, as the link allows.
pub const MAX_PACKAGE_BYTES: usize = 255;
/// The longest app label kept, in characters: 120, as the link allows.
pub const MAX_APP_LABEL_CHARS: usize = 120;
/// The longest hash of an app list, in hex digits.
pub const MAX_APPS_HASH_DIGITS: usize = 64;

/// After how many minutes with no keyboard or mouse input the count pauses
/// on the master PC, when the guardian has not chosen: 3.
pub const DEFAULT_IDLE_MINUTES: u32 = 3;
/// The fewest idle minutes that can be set: 1.
pub const MIN_IDLE_MINUTES: u32 = 1;
/// The most idle minutes that can be set: 60.
pub const MAX_IDLE_MINUTES: u32 = 60;

/// How many days of usage are kept slice by slice when the guardian has not
/// chosen: 35. Older days are folded into daily totals by [`Hub::tidy`].
pub const DEFAULT_DETAIL_DAYS: u32 = 35;
/// The fewest days of usage that can be kept slice by slice: 7.
pub const MIN_DETAIL_DAYS: u32 = 7;
/// The most days of usage that can be kept slice by slice: 400.
pub const MAX_DETAIL_DAYS: u32 = 400;

/// How many home days each child's book in the core keeps: today and
/// yesterday. The core needs no more (a grant is only made for a device
/// whose day is within one of the master's, and a request can only be
/// answered while its day lasts), and the usage the hub shows lives in its
/// own tables. See the README.
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

/// Events older than this many days are dropped by [`Hub::tidy`]: 90.
pub const EVENT_KEEP_DAYS: i64 = 90;
/// At most this many events are kept, the newest: 2,000.
pub const MAX_EVENTS: u32 = 2_000;

/// The longest range of days [`Hub::usage`] answers for at once: 400.
pub const MAX_USAGE_DAYS: u32 = 400;

/// The most events [`Hub::events`] returns at once: 500.
pub const MAX_EVENTS_PER_CALL: u32 = 500;

/// The shortest guardian PIN, in digits: 6, by the owner's decision, as the
/// core has it. The hub refuses a shorter PIN itself, before the core sees
/// it.
pub const MIN_PIN_DIGITS: u32 = limo_core::MIN_PIN_DIGITS;
/// The longest guardian PIN, in digits: the core's limit, 8.
pub const MAX_PIN_DIGITS: u32 = limo_core::MAX_PIN_DIGITS;

/// The version of this crate, for example "0.1.0".
pub fn hub_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
