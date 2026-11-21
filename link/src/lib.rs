//! # LiMo link
//!
//! The encrypted link between a phone and the family's master PC, on the
//! home network. There is no server: a phone talks to one PC, and only to
//! it.
//!
//! This crate has no sockets, no threads, no clocks and no discovery calls
//! in it. The callers own those: the Windows service on the master, and the
//! phone's Kotlin through a wrapper. The crate gives them:
//!
//! - the frames ([`frame`]): a 4 byte length and a body, every limit
//!   checked from the length before anything is kept;
//! - the two handshakes and the session after them, as state machines fed
//!   with bytes and the time: on the phone [`PhonePairing`] and
//!   [`PhoneSession`], on the master [`MasterLink`];
//! - the messages ([`messages`]), JSON inside Noise, carrying the core's
//!   [`limo_core::Report`] and [`limo_core::Grant`];
//! - the pairing offer ([`Offer`]): making one, and writing and reading its
//!   `limo1:` text form;
//! - the discovery names and TXT values ([`discovery`]) as plain functions;
//! - the timing numbers as constants.
//!
//! Pairing runs `Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s` with the offer's
//! secret as the pre-shared key; every session after it runs
//! `Noise_KK_25519_ChaChaPoly_BLAKE2s` with both static keys pinned. The
//! README next to this crate says what each one promises and what it does
//! not, with where that was read.
//!
//! Private keys, offer secrets and code secrets never show in `Debug`
//! output or in an error, and are wiped from memory when dropped. The crate
//! has no unsafe code.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod crypto;
#[cfg(feature = "dev")]
pub mod dev;
pub mod discovery;
mod encoding;
mod error;
pub mod frame;
mod keys;
mod master;
pub mod messages;
mod offer;
mod phone;

pub use discovery::{Announced, DEFAULT_PORT, MasterId, TxtProblem};
pub use error::LinkError;
pub use frame::MAX_MESSAGE_BYTES;
pub use keys::{KEY_BYTES, KeyPair, PublicKey};
pub use master::{Directory, DirectoryList, KnownPhone, MasterEvent, MasterLink, OfferOnFile};
pub use messages::{AppEntry, CodeSecret, Form, PairRequest, PhoneAbout, Refusal, apps_hash};
pub use offer::{MAX_OFFER_ADDRESSES, OFFER_LIFETIME_SECS, OFFER_PREFIX, Offer, OfferId, OfferProblem};
pub use phone::{PairedMaster, PhoneEvent, PhonePairing, PhoneSession, Welcomed};

/// The version of the link this crate speaks. It goes in the clear at the
/// start of every link, in the handshake's prologue, and in the `v` TXT
/// record. A later version keeps the frames, the handshakes and the
/// `refused` message as they are, and may add messages, which a side that
/// does not know them passes over; a master that meets a phone of a version
/// it does not speak finishes the handshake and refuses it with
/// [`Refusal::Version`].
pub const LINK_VERSION: u8 = 1;

/// The handshake must finish within 5 seconds of the connection, on both
/// sides. On the master that includes the phone's first message after it.
pub const HANDSHAKE_LIMIT_MS: u64 = 5_000;

/// A phone sends a ping when it has sent nothing for 60 seconds.
pub const PING_EVERY_MS: u64 = 60_000;

/// The master drops a session it has heard nothing on for 180 seconds.
pub const SILENCE_LIMIT_MS: u64 = 180_000;

/// A report, a ping or a pairing request not answered within 10 seconds
/// ends the link. For a report, the phone then takes it that the master
/// cannot be reached (`DeviceState::lost_contact`).
pub const ANSWER_LIMIT_MS: u64 = 10_000;

/// The most questions (reports and pings) a phone has waiting for answers
/// at once.
pub const MAX_WAITING: usize = 8;

/// The time, as the caller's clocks give it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    /// A steady count of milliseconds that never goes back while the link
    /// lasts: `SystemClock.elapsedRealtime()` on Android, the count since
    /// the PC started on Windows. Every limit of the link is measured on it.
    pub ms: u64,
    /// The wall clock, unix seconds. The master puts it in its messages and
    /// checks an offer's expiry with it; a phone's link never reads it.
    pub unix: i64,
}

/// The version of this crate, for example "0.1.0".
pub fn link_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
