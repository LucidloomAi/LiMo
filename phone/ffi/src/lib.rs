//! # LiMo phone bridge
//!
//! The thin layer between the Kotlin of LiMo's Android app (the child form)
//! and the Rust that decides: the phone's engine (`limo_phone`) and the
//! phone's side of the link to the master PC (`limo_link`). UniFFI
//! generates the Kotlin that calls it.
//!
//! What crosses, and in which shape (the README next to this crate has the
//! whole list, value by value):
//!
//! - **objects with methods**: [`PhoneEngine`], and for the link [`Offer`],
//!   [`PairedMaster`], [`PhonePairing`] and [`PhoneSession`]. Each wraps the
//!   crate's own value behind a lock, so Kotlin may call it from any thread;
//! - **plain records, lists and optional values**: the facts in, the orders
//!   out, and the answers to the ways out;
//! - **records with a kind and optional fields** for every Rust enum that
//!   carries data (what is in front, the cover, a code's outcome, a link
//!   event, a PIN's action), with Kotlin enums for the kinds (a guarded
//!   page among them);
//! - **bytes Kotlin never reads**: the core's report and grant, as JSON,
//!   carried between the engine and the link; the kept state; the kept
//!   pairing;
//! - **flat errors**: [`EngineError`] and [`LinkError`], one exception class
//!   per variant in Kotlin, with plain words as the message.
//!
//! No callbacks run from Rust into Kotlin, and no function is async.
//!
//! **Unsafe code.** UniFFI's macros generate unsafe code (the exported C
//! functions and the buffers that cross), so this crate cannot forbid it as
//! the others do. No line written here by hand is unsafe; the conversions
//! live in a module that forbids it, and a test checks the sources. Nothing
//! here decides a rule: every decision, word and number is the engine's or
//! the link's.
//!
//! **Secrets.** A typed code or PIN, a code secret, an offer's text and the
//! kept pairing cross as byte arrays; this layer wipes its own copy as soon
//! as it is done with it. What the Kotlin side must do with its copy, and
//! what is not wiped, is in the README.
//!
//! With the `testkit` feature (tests on the PC only, never on a phone),
//! `testkit::TestFamily` plays the master PC.

#![warn(missing_docs)]

uniffi::setup_scaffolding!();

mod convert;
mod engine;
mod link;
mod shapes;
#[cfg(feature = "testkit")]
pub mod testkit;

pub use engine::PhoneEngine;
pub use link::{Offer, PairedMaster, PhonePairing, PhoneSession};
pub use shapes::{
    AppEntry, BlockCover, CallState, Cover, CoverKind, EngineError, Front, FrontKind, GrantOutcome, GrantTaken,
    GuardCover, GuardedPage, LinkError, LinkEvent, LinkEventKind, LoadProblem, LoadProblemKind, LockCover, LockWhy,
    NothingToDo, PairRequest,
    PhoneFacts, PhoneOrders, PhonePermissions, PhoneView, PinAction, PinActionKind, RequestAnswer, RequestLine,
    RequestShown, TimeLeft, TimeRequest, TryAnswer, TryKind, Versions, WarningCause, WarningShown, Way, WayRow,
};

/// The versions of what is inside the library: this layer, the engine, the
/// core, the link crate, and the version of the link it speaks.
#[uniffi::export]
pub fn versions() -> Versions {
    Versions {
        ffi: env!("CARGO_PKG_VERSION").to_string(),
        engine: limo_phone::phone_engine_version(),
        core: limo_core::core_version(),
        link: limo_link::link_version(),
        link_protocol: i32::from(limo_link::LINK_VERSION),
    }
}
