//! The messages between a device and the master, as plain data.
//!
//! There are two:
//!
//! - a [`Report`] goes from a device to the master. It says what the device
//!   holds and has used, hands over its usage [`Slice`]s, acknowledges the
//!   newest grant and rules it has taken, and may ask for more time;
//! - a [`Grant`] goes back. It says how much lease the device may have in
//!   all for the day, its away allowance, the current rules, and which slices
//!   are now counted.
//!
//! Every figure in them is a level ("your lease for today is 1800 seconds in
//! all"), never a change ("here are 600 more seconds"), and every message
//! carries a number that only grows. So a message that is lost, repeated or
//! late can never make anything count twice or go missing. How the messages
//! travel, and their encryption, are not this library's concern.
//!
//! The ways out ride on the same two messages. A report also carries the
//! time added on the device by codes and the PIN, and the child's requests
//! for more time, each with an id and repeated until confirmed, like slices.
//! A grant also carries the guardian's lock or unlock, the PIN record while
//! the device may not have its newest version, and the day's requests with
//! their answers.
//!
//! A report also carries the device's notes for the guardian
//! ([`crate::notes`]), with ids and repeated until confirmed, like slices;
//! and a grant the use on the child's other devices that day, so a phone can
//! say where the day's time went.

use serde::{Deserialize, Serialize};

use crate::extra::{AddedTime, RequestView, TimeRequest};
use crate::guardian::{GuardianSetting, PinUnlock};
use crate::notes::SentNote;
use crate::pin::PinUpdate;
use crate::rules::RuleSet;

/// A stretch of use of one app on one device, inside one home day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slice {
    /// Unique for the child: the device id, a slash and a number that only
    /// grows. A slice handed over twice is counted once.
    pub id: String,
    /// The child.
    pub child_id: String,
    /// The device it was used on.
    pub device_id: String,
    /// The app in front. Opaque; may be empty when not known.
    pub app_id: String,
    /// The home day it belongs to, as the device counted it. A stretch that
    /// crosses a day boundary is cut into one slice per day.
    pub day: i64,
    /// Start, unix seconds.
    pub start: i64,
    /// End, unix seconds, after the start.
    pub end: i64,
    /// True when the time counts against the budget: false for an always
    /// allowed app, as the device's rules said when it was used.
    pub counts: bool,
}

impl Slice {
    /// Its length in seconds.
    pub fn secs(&self) -> u64 {
        self.end.saturating_sub(self.start).max(0) as u64
    }
}

/// From a device to the master: what it holds and has used, its usage, and
/// whether it would like more time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    /// The child.
    pub child_id: String,
    /// The device sending it.
    pub device_id: String,
    /// Grows by one with every report this device sends. The master keeps the
    /// figures of the highest one it has seen.
    pub message_number: u64,
    /// The device's clock when it was sent, unix seconds. For the record.
    pub sent_at: i64,
    /// The version of the rules the device holds. Acknowledges them.
    pub rules_version: u32,
    /// The number of the newest grant the device has taken (0 for none).
    /// Acknowledges it, and tells the master that older grants it did not
    /// take will never be taken.
    pub grant_number: u64,
    /// The away share, in percent, the device holds for later days.
    pub away_percent: u8,
    /// The home day the figures below are for.
    pub day: i64,
    /// Counted use on this device that day, in seconds.
    pub used_secs: u64,
    /// The lease the device has taken for that day, in all.
    pub lease_total_secs: u64,
    /// The device's away allowance for that day.
    pub away_cap_secs: u64,
    /// The part of its use that came from the away allowance.
    pub away_used_secs: u64,
    /// True to ask for the lease to be topped up to one lease length.
    pub want_more: bool,
    /// Every slice the master has not yet confirmed.
    pub slices: Vec<Slice>,
    /// Time added on this device that day by codes and the PIN, in all.
    #[serde(default)]
    pub added_secs: u64,
    /// The part of it used.
    #[serde(default)]
    pub added_used_secs: u64,
    /// Every piece of time added on the device that the master has not yet
    /// confirmed.
    #[serde(default)]
    pub added: Vec<AddedTime>,
    /// Every request for more time made on the device that the master has
    /// not yet confirmed.
    #[serde(default)]
    pub requests: Vec<TimeRequest>,
    /// Wrong guesses of codes and PIN on this device since the last right
    /// PIN, for the guardian to see.
    #[serde(default)]
    pub wrong_guesses: u32,
    /// The version of the PIN record the device holds, 0 for none.
    /// Acknowledges it.
    #[serde(default)]
    pub pin_version: u32,
    /// The guardian news number of the newest grant the device has taken.
    /// Acknowledges the lock or unlock, the PIN and the answers it carried.
    #[serde(default)]
    pub news_seen: u64,
    /// An unlock set with the PIN on this device, while it is in force.
    #[serde(default)]
    pub pin_unlock: Option<PinUnlock>,
    /// Every note for the guardian the master has not yet confirmed, oldest
    /// first, each with its id ([`SentNote`]). A device that makes no notes
    /// leaves it empty. Reading a report leaves out a note of a kind this
    /// version does not know, so a newer device's report is still taken.
    #[serde(default, deserialize_with = "crate::notes::read_notes", skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<SentNote>,
}

/// From the master to a device: what it may hold, the rules, and which
/// slices are counted. Also the first message a device gets, at pairing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// The child.
    pub child_id: String,
    /// The device it is for.
    pub device_id: String,
    /// Grows with every grant the master makes. A device takes a grant only
    /// if its number is higher than any it has taken.
    pub number: u64,
    /// The report it answers (0 for the grant made at pairing).
    pub answers_message: u64,
    /// The home day the lease and the away allowance are for.
    pub day: i64,
    /// The lease the device may have for that day, in all, in seconds.
    pub lease_total_secs: u64,
    /// The away allowance for that day, in seconds.
    pub away_cap_secs: u64,
    /// The away share, in percent of a day's budget, for the device to use
    /// when a new day starts before it hears from the master again.
    pub away_percent: u8,
    /// The time left for this device that day, if no other device used any:
    /// what it may hold after this grant plus what the master has not handed
    /// out. None when the day has no limit. For showing and for warnings.
    pub left_for_you_secs: Option<u64>,
    /// Away time other devices hold that day and have not used, as far as
    /// the master knows: time that may come to this device when they report
    /// from home. Lets a device that has spent what it holds say it is
    /// waiting rather than that the day is used up.
    pub held_elsewhere_secs: u64,
    /// The device's use that day as the master had it when it made this
    /// grant, so the device can bring `left_for_you_secs` up to date.
    pub used_secs_reported: u64,
    /// True when this grant lowers the device's away allowance below what the
    /// master holds back for it: the device should report again at once, so
    /// that the time is free for a device that is waiting.
    pub acknowledge_now: bool,
    /// The current rules. A device takes them if their version is higher than
    /// the one it holds.
    pub rules: RuleSet,
    /// Ids of slices the master now has. The device can forget them.
    pub confirmed_slices: Vec<String>,
    /// Ids of time added on the device that the master now has.
    #[serde(default)]
    pub confirmed_added: Vec<String>,
    /// Ids of requests the master now has.
    #[serde(default)]
    pub confirmed_requests: Vec<String>,
    /// The guardian's lock or unlock for all of the child's devices.
    #[serde(default)]
    pub guardian: GuardianSetting,
    /// The PIN record, while the device may not hold its newest version.
    /// None means no change, not no PIN.
    #[serde(default)]
    pub pin: Option<PinUpdate>,
    /// Every request for more time made for the grant's day, from any of the
    /// child's devices, with its answer so far.
    #[serde(default)]
    pub requests: Vec<RequestView>,
    /// Grows whenever the guardian's lock or unlock, the PIN or an answer to
    /// a request changes. A device reports the newest it has taken.
    #[serde(default)]
    pub news: u64,
    /// Ids of notes the master now has. The device can forget them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confirmed_notes: Vec<String>,
    /// Counted use that day on the child's other devices, those since
    /// removed among them, as the master had it when it made this grant, in
    /// seconds. With the device's own use it is the child's whole day, so a
    /// phone can say how much of today went on the PC. None from a master
    /// that does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub others_used_secs: Option<u64>,
    /// True when all of that use, a second or more, was on the master PC
    /// (the child's device of kind [`crate::DeviceKind::OnMaster`]); false
    /// when there was none, or some was on another device.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub others_all_on_master: bool,
}
