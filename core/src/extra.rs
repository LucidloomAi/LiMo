//! Extra time, and the requests that ask for it.
//!
//! Every piece of extra time is a record with an id, so the same piece given
//! or reported twice counts once, and the master can show where a day's extra
//! came from ([`ExtraTime`], [`ExtraSource`]). It comes four ways:
//!
//! - a guardian gives it at the master;
//! - a guardian grants a child's request ([`TimeRequest`]), with the minutes
//!   asked for or another number;
//! - a one time code adds it on a device ([`AddedTime`]);
//! - the guardian PIN adds it on a device.
//!
//! Time given at the master goes into the day's pool, and leases hand it out
//! like any other time. Time added on a device is that device's to spend at
//! once, even with no network; the device reports it, and the master adds it
//! to that day's extra, once, by its id. When that device is removed, the
//! part it had not used is dropped ([`ExtraTime::dropped_secs`]).

use serde::{Deserialize, Serialize};

/// The most minutes a guardian can give or add at once: a whole day.
pub const MAX_EXTRA_MINUTES: u32 = 1_440;
/// The most minutes a child can ask for at once: four hours.
pub const MAX_ASK_MINUTES: u32 = 240;
/// The longest reason a child can give with a request, in characters.
pub const MAX_REASON_CHARS: u32 = 200;
/// How many requests a device can make in one home day.
pub const MAX_REQUESTS_PER_DAY: u32 = 5;

/// Where a piece of extra time came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtraSource {
    /// Given by a guardian at the master.
    Guardian,
    /// A child's request, granted.
    Request {
        /// The request.
        request_id: String,
    },
    /// A one time code typed on a device.
    Code {
        /// The device.
        device_id: String,
        /// The code's selector: the minutes' index times ten plus the
        /// counter.
        selector: u8,
    },
    /// The guardian PIN typed on a device.
    Pin {
        /// The device.
        device_id: String,
    },
}

/// One piece of a day's extra time, as the master records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraTime {
    /// Its id. The same id is counted once.
    pub id: String,
    /// The home day it is for.
    pub day: i64,
    /// How much, in seconds.
    pub secs: u64,
    /// Where it came from.
    pub source: ExtraSource,
    /// When it was given, or added on the device, unix seconds.
    pub at: i64,
    /// Of time added on a device, the part the device had not used when it
    /// was removed: dropped from the day's extra time, since no other device
    /// may spend it. 0 otherwise. The day's extra time counts `secs` less
    /// this.
    #[serde(default)]
    pub dropped_secs: u64,
}

/// How time was added on a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddedBy {
    /// A one time code.
    Code {
        /// The code's selector.
        selector: u8,
    },
    /// The guardian PIN.
    Pin,
}

/// Time added on a device by a code or the PIN, as the device reports it.
/// It travels with every report until the master confirms it, like a slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddedTime {
    /// Unique for the child: the device id, then `/code/`, the day and the
    /// selector for a code, or `/pin/` and a number that only grows for the
    /// PIN.
    pub id: String,
    /// The child.
    pub child_id: String,
    /// The device.
    pub device_id: String,
    /// The home day it is for: the device's day when it was added.
    pub day: i64,
    /// How much, in seconds.
    pub secs: u64,
    /// How.
    pub by: AddedBy,
    /// When, by the device's clock, unix seconds.
    pub at: i64,
}

/// A child's request for more time, made on a device. It travels with every
/// report until the master confirms it, like a slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRequest {
    /// Unique for the child: the device id, `/ask/` and a number that only
    /// grows.
    pub id: String,
    /// The child.
    pub child_id: String,
    /// The device it was made on.
    pub device_id: String,
    /// The home day it asks for time on.
    pub day: i64,
    /// The minutes asked for: 1 to [`MAX_ASK_MINUTES`].
    pub minutes: u32,
    /// The child's reason, possibly empty: at most [`MAX_REASON_CHARS`]
    /// characters.
    pub reason: String,
    /// When it was made, by the device's clock, unix seconds.
    pub asked_at: i64,
}

/// The answer to a request, so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestAnswer {
    /// Not answered yet.
    Waiting,
    /// Granted, with this many minutes (which may differ from those asked
    /// for).
    Granted {
        /// The minutes given.
        minutes: u32,
    },
    /// Refused.
    Refused,
}

/// A guardian's reply to a request, at the master.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestReply {
    /// Grant this many minutes: 1 to [`MAX_EXTRA_MINUTES`].
    Grant {
        /// The minutes to give.
        minutes: u32,
    },
    /// Refuse.
    Refuse,
}

/// A request and how it stands, for the guardian's screens and for every
/// device's lock screen. Grants carry those of the grant's day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestView {
    /// The request.
    pub request: TimeRequest,
    /// Its answer so far.
    pub answer: RequestAnswer,
    /// When it was answered, by the master's clock, unix seconds.
    pub answered_at: Option<i64>,
    /// False while the request has not yet reached the master: shown only on
    /// the device it was made on.
    pub reached_master: bool,
}
