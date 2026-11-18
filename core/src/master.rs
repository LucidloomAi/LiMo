//! The master side: one child's pool, the devices, and the usage record.
//!
//! The master keeps, for each day, a pool: the day's budget plus any extra
//! time, less the use it has been told about. It never hands the pool out
//! whole. A device in reach takes a small lease, spends it, reports and asks
//! again. A phone also holds an away allowance for when it cannot reach the
//! master.
//!
//! The master **reserves**, for every device, all that device might still
//! spend without telling it: the lease it may hold, and its away allowance.
//! It works this out from the newest report the device sent and the grants
//! sent since, which the device may or may not have received. Reserved time
//! is not handed to anyone else. A reservation shrinks only when a report
//! shows what was really used, or when the device's day is over.
//!
//! A phone at home does not keep its away allowance from the others. When a
//! device asks for time the pool cannot give, the master remembers the
//! shortfall as an unmet need, and lowers the away allowance of the next
//! phone that reports, never below what that phone has used away. Once the
//! phone acknowledges the lower figure, the time is free for the device that
//! needed it. A phone out of reach never acknowledges, so nothing is taken
//! from a phone that cannot hear. [`MasterBook::wanted_reports`] names the
//! devices worth asking to report at once.
//!
//! With the same rules all day, the time counted for a day can never pass
//! the budget plus extra time: see the crate's README for the proof and its
//! limits.
//!
//! The master also keeps what a guardian can do from it: extra time, each
//! piece with an id ([`MasterBook::give_extra`]); the children's requests and
//! their answers ([`MasterBook::answer_request`]); the code secret of each
//! device ([`MasterBook::set_code_secret`]); the PIN record
//! ([`MasterBook::set_pin`]); and a lock or an unlock for all of a child's
//! devices ([`MasterBook::lock_now`], [`MasterBook::unlock_until`]). Time a
//! device added on its own, with a code or the PIN, reaches the master with
//! the device's reports and joins the day's extra time once, by its id.
//!
//! The book is small. It does not keep the slices of use themselves: for
//! each device and day it keeps the highest slice number it has taken and
//! the counted seconds. [`MasterBook::handle_report`] returns the slices,
//! pieces of added time and requests it took for the first time
//! ([`ReportOutcome`]), so that the caller can keep them, once. A device is
//! removed with [`MasterBook::remove_device`]: its counted use stays with the
//! days, and what was held back for it is free at once.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::codes::{CODE_SECRET_BYTES, Secret, check_secret};
use crate::extra::{
    AddedBy, AddedTime, ExtraSource, ExtraTime, MAX_EXTRA_MINUTES, MAX_REQUESTS_PER_DAY, RequestAnswer,
    RequestReply, RequestView, TimeRequest,
};
use crate::guardian::{GuardianMode, GuardianSetting, MAX_UNLOCK_SECS, PinUnlock};
use crate::messages::{Grant, Report, Slice};
use crate::pin::{PinError, PinRecord, PinUpdate};
use crate::rules::{RuleProblem, RuleSet};
use crate::time::SECONDS_PER_DAY;

/// What kind of device it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    /// The PC that runs the master. Always in reach, so it holds no away
    /// allowance.
    OnMaster,
    /// A device that may be carried out of reach of the master, such as a
    /// phone. It holds an away allowance.
    Portable,
}

/// Errors from the master.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MasterError {
    /// The rules have problems and were not taken.
    InvalidRules {
        /// Every problem found.
        problems: Vec<RuleProblem>,
    },
    /// New rules must have a higher version than the current ones.
    RulesNotNewer {
        /// The current version.
        current: u32,
        /// The version given.
        given: u32,
    },
    /// No device with this id is paired for this child.
    UnknownDevice {
        /// The id given.
        device_id: String,
    },
    /// A device with this id is already paired for this child.
    DeviceAlreadyPaired {
        /// The id given.
        device_id: String,
    },
    /// The message is for another child.
    WrongChild {
        /// This book's child.
        expected: String,
        /// The child named in the message.
        given: String,
    },
    /// A code secret must be exactly 32 bytes.
    SecretWrongLength {
        /// The length given, in bytes.
        given: u32,
    },
    /// The PIN record could not be made or taken.
    BadPin {
        /// Why.
        problem: PinError,
    },
    /// The minutes are outside what is allowed: 1 to 1440.
    MinutesOutOfRange {
        /// The minutes given.
        minutes: u32,
    },
    /// A lock must end later than now; an unlock must end later than now and
    /// at most a day ahead.
    TimeOutOfRange,
    /// No request with this id has reached the master.
    UnknownRequest {
        /// The id given.
        request_id: String,
    },
    /// The request was answered already. A request is answered once.
    AlreadyAnswered {
        /// The id given.
        request_id: String,
    },
    /// The request was for a day that is over.
    RequestTooLate {
        /// The id given.
        request_id: String,
    },
    /// The device was removed from this child. Its reports are refused, and
    /// its id is never paired again: a device paired anew gets a new id.
    DeviceRemoved {
        /// The id given.
        device_id: String,
    },
}

impl fmt::Display for MasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MasterError::InvalidRules { problems } => {
                write!(f, "The rules were not taken:")?;
                for p in problems {
                    write!(f, " {}", p.message)?;
                }
                Ok(())
            }
            MasterError::RulesNotNewer { current, given } => write!(
                f,
                "New rules must have a version above {current}, the current one; they have {given}."
            ),
            MasterError::UnknownDevice { device_id } => {
                write!(f, "No device called {device_id} is paired for this child.")
            }
            MasterError::DeviceAlreadyPaired { device_id } => {
                write!(f, "A device called {device_id} is already paired for this child.")
            }
            MasterError::WrongChild { expected, given } => {
                write!(f, "This message is for {given}, but this book is for {expected}.")
            }
            MasterError::SecretWrongLength { given } => write!(
                f,
                "A code secret must be {CODE_SECRET_BYTES} bytes long; this one is {given} bytes."
            ),
            MasterError::BadPin { problem } => write!(f, "The PIN was not set: {problem}"),
            MasterError::MinutesOutOfRange { minutes } => write!(
                f,
                "{minutes} minutes cannot be given: it must be from 1 to {MAX_EXTRA_MINUTES}."
            ),
            MasterError::TimeOutOfRange => write!(
                f,
                "A lock must end later than now, and an unlock later than now and at most a day ahead."
            ),
            MasterError::UnknownRequest { request_id } => {
                write!(f, "No request called {request_id} has reached this book.")
            }
            MasterError::AlreadyAnswered { request_id } => {
                write!(f, "The request {request_id} has been answered already.")
            }
            MasterError::RequestTooLate { request_id } => write!(
                f,
                "The request {request_id} was for a day that is over. Give extra time for today instead."
            ),
            MasterError::DeviceRemoved { device_id } => write!(
                f,
                "The device called {device_id} was removed from this child, so nothing from it is taken. Pair it again under a new id to use it."
            ),
        }
    }
}

impl std::error::Error for MasterError {}

/// One day's pool, as the master sees it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolView {
    /// The home day.
    pub day: i64,
    /// The day's budget under the current rules, in seconds. None when the
    /// day has no limit.
    pub budget_secs: Option<u64>,
    /// Extra time given for this day, in seconds.
    pub extra_secs: u64,
    /// Counted use the master has been told about, in seconds.
    pub used_secs: u64,
    /// Time reserved for devices: what they may still spend without telling
    /// the master. None when a device may hold rules with no limit for this
    /// day and could spend any amount.
    pub reserved_secs: Option<u64>,
    /// What the master can still hand out. None when there is no limit.
    pub free_secs: Option<u64>,
    /// By how much use and reservations together pass the budget plus extra
    /// time. Normally 0; above 0 after the budget was cut, or after use while
    /// a guardian's unlock kept a device open with nothing in hand.
    pub over_secs: u64,
}

/// One device's part of a day, as the master sees it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceView {
    /// The device.
    pub device_id: String,
    /// Its kind.
    pub kind: DeviceKind,
    /// Its counted use that day that the master knows of, in seconds.
    pub used_secs: u64,
    /// What it may still spend that day without telling the master. None
    /// when it may hold rules with no limit for that day.
    pub reserved_secs: Option<u64>,
    /// Away time it will give back when it acknowledges its newest grant,
    /// which lowered its away allowance less than [`NEED_KEPT_SECS`] ago.
    pub giving_back_secs: u64,
    /// When the master last heard from it, unix seconds.
    pub last_seen: Option<i64>,
    /// Time it asked for that day and could not be given yet, in seconds.
    pub waiting_secs: u64,
    /// Time added on it that day by codes and the PIN, as far as the master
    /// knows, in seconds.
    #[serde(default)]
    pub added_secs: u64,
    /// Wrong guesses of codes and PIN on it, as its newest report said.
    #[serde(default)]
    pub wrong_guesses: u32,
    /// When an unlock set with the PIN on it ends, as its newest report said.
    #[serde(default)]
    pub pin_unlocked_until: Option<i64>,
    /// The rules version it holds as far as the master knows: what its
    /// newest report said, or, before it has reported, the version it was
    /// paired with.
    #[serde(default)]
    pub rules_version: u32,
    /// True when it has taken the latest news: the current rules, and the
    /// guardian's newest lock or unlock, PIN and answers to requests. When
    /// false, it is in [`MasterBook::wanted_reports`].
    #[serde(default)]
    pub has_latest_news: bool,
    /// How far its clock was from the master's at its newest report, in
    /// seconds: the report's sending time less the time the master was given
    /// with it. Ahead is positive. None before its first report (or for a
    /// report taken by a book saved before this was kept).
    #[serde(default)]
    pub clock_off_by_secs: Option<i64>,
}

/// What [`MasterBook::handle_report`] did with a report: the grant to send
/// back, and what the book took from the report for the first time. A caller
/// that keeps a record of its own (usage, requests) stores exactly these,
/// and so stores each piece once, however often the device sends it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportOutcome {
    /// The grant to send back. Save the book before sending it.
    pub grant: Grant,
    /// The slices taken for the first time, counted or not (a slice of an
    /// always allowed app is taken but does not count), in the report's
    /// order.
    pub new_slices: Vec<Slice>,
    /// The pieces of time added on the device by a code or the PIN that
    /// joined the day's extra time with this report.
    pub new_added: Vec<AddedTime>,
    /// The requests for more time that reached the book with this report.
    pub new_requests: Vec<TimeRequest>,
    /// True when the report was the newest from its device, so that its
    /// figures (what it holds, its wrong guesses, its clock) were taken.
    /// False for a report that came late or again.
    pub newest: bool,
}

/// A device's counted use on one day, as the book keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountedUse {
    /// The device.
    pub device_id: String,
    /// Its counted use that day, in seconds.
    pub counted_secs: u64,
    /// True when the device has since been removed. Its use stays counted
    /// for the days the book keeps.
    pub removed: bool,
}

/// The figures of the newest report from a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LastReport {
    message_number: u64,
    day: i64,
    rules_version: u32,
    grant_number: u64,
    away_percent: u8,
    used_secs: u64,
    lease_total_secs: u64,
    away_cap_secs: u64,
    away_used_secs: u64,
    #[serde(default)]
    added_secs: u64,
    #[serde(default)]
    added_used_secs: u64,
    #[serde(default)]
    wrong_guesses: u32,
    #[serde(default)]
    pin_version: u32,
    #[serde(default)]
    news_seen: u64,
    #[serde(default)]
    pin_unlock: Option<PinUnlock>,
    /// The report's sending time less the master's time when it arrived.
    #[serde(default)]
    clock_off_by: Option<i64>,
}

impl LastReport {
    fn from_report(r: &Report, now: i64) -> LastReport {
        LastReport {
            message_number: r.message_number,
            day: r.day,
            rules_version: r.rules_version,
            grant_number: r.grant_number,
            away_percent: r.away_percent,
            used_secs: r.used_secs,
            lease_total_secs: r.lease_total_secs,
            away_cap_secs: r.away_cap_secs,
            away_used_secs: r.away_used_secs,
            added_secs: r.added_secs,
            added_used_secs: r.added_used_secs,
            wrong_guesses: r.wrong_guesses,
            pin_version: r.pin_version,
            news_seen: r.news_seen,
            pin_unlock: r.pin_unlock,
            clock_off_by: Some(r.sent_at.saturating_sub(now)),
        }
    }

    fn lease_used_secs(&self) -> u64 {
        self.used_secs.saturating_sub(self.away_used_secs).saturating_sub(self.added_used_secs)
    }
}

/// A grant the device has not yet acknowledged. It may have arrived or not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SentGrant {
    number: u64,
    day: i64,
    lease_total_secs: u64,
    away_cap_secs: u64,
    away_percent: u8,
    rules_version: u32,
    /// When it was made, unix seconds.
    #[serde(default)]
    sent_at: i64,
}

/// What a report asked for, as far as the grant is concerned.
#[derive(Debug, Clone, Copy, Default)]
struct Ask {
    /// Top the lease up.
    want_more: bool,
    /// The report is the device's newest (or a repeat of it), so it says
    /// what the device still needs.
    current: bool,
    /// The report's number (0 for the grant made at pairing).
    answers_message: u64,
}

/// How long an unmet need is kept without being asked for again, in
/// seconds. A device that is waiting asks again every minute
/// ([`crate::decide::RETRY_SECS`]); one that went idle or out of reach stops,
/// and its need lapses, so that time is not kept for it.
///
/// A lowered away allowance that has not been acknowledged is counted on for
/// as long: a phone that does not acknowledge in that time may have left, and
/// the next phone at home is asked instead.
pub const NEED_KEPT_SECS: i64 = 180;

/// Time a device asked for and could not be given, waiting to be met.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Need {
    day: i64,
    secs: u64,
    /// The grant number current when the need arose: older needs are met
    /// first.
    since: u64,
    /// When it was last asked for, unix seconds.
    at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DeviceRecord {
    device_id: String,
    kind: DeviceKind,
    last_report: Option<LastReport>,
    /// Grants with a number above the newest one the device acknowledged.
    unacknowledged: Vec<SentGrant>,
    last_seen: Option<i64>,
    /// What it asked for at its newest report and could not be given.
    #[serde(default)]
    need: Option<Need>,
    /// Its code secret, or empty.
    #[serde(default)]
    code_secret: Secret,
    /// The rules version of the grant it was paired with: it made its state
    /// from that grant, so it holds those rules before it ever reports.
    #[serde(default)]
    paired_rules: u32,
    /// The guardian news number of that grant.
    #[serde(default)]
    paired_news: u64,
}

impl DeviceRecord {
    /// The rules version it holds as far as the master knows.
    fn rules_held(&self) -> u32 {
        self.last_report.as_ref().map_or(self.paired_rules, |r| r.rules_version.max(self.paired_rules))
    }

    /// The newest guardian news it has taken as far as the master knows.
    fn news_held(&self) -> u64 {
        self.last_report.as_ref().map_or(self.paired_news, |r| r.news_seen.max(self.paired_news))
    }

    /// Every (away percent, rules version) pair the device may hold: the one
    /// it last reported and those of the grants it has not acknowledged.
    fn possible_terms(&self) -> Vec<(u8, u32)> {
        let mut terms: Vec<(u8, u32)> =
            self.unacknowledged.iter().map(|g| (g.away_percent, g.rules_version)).collect();
        if let Some(r) = &self.last_report {
            terms.push((r.away_percent, r.rules_version));
        }
        terms
    }

    fn highest_possible_percent(&self) -> u8 {
        self.possible_terms().iter().map(|(p, _)| *p).max().unwrap_or(0)
    }

    /// Its unmet need on `day`, if any and still asked for at `clock`.
    fn need_on(&self, day: i64, clock: i64) -> Option<Need> {
        self.need
            .filter(|n| n.day == day && n.secs > 0 && n.at.saturating_add(NEED_KEPT_SECS) >= clock)
    }
}

/// One day of the book. It does not keep the slices themselves: for each
/// device it keeps the highest slice number taken for the day (its mark)
/// and the counted seconds. Why a mark is enough to count every slice once
/// is set out at [`MasterBook::handle_report`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "SavedDay")]
struct DayRecord {
    day: i64,
    /// Extra time given without an id, by a book saved before extra time
    /// had ids.
    extra_secs: u64,
    /// The highest slice number taken from each live device for this day.
    marks: BTreeMap<String, u64>,
    /// Counted seconds per live device.
    counted: BTreeMap<String, u64>,
    /// Counted seconds of devices since removed, by id. They stay counted
    /// against the day's pool.
    removed: BTreeMap<String, u64>,
    /// Every piece of extra time for this day, by a key made of where it
    /// came from and its id, so that ids from different sources never meet.
    extras: BTreeMap<String, ExtraTime>,
    /// Every request for more time for this day, by id.
    requests: BTreeMap<String, RequestRecord>,
}

impl DayRecord {
    fn new(day: i64) -> DayRecord {
        DayRecord {
            day,
            extra_secs: 0,
            marks: BTreeMap::new(),
            counted: BTreeMap::new(),
            removed: BTreeMap::new(),
            extras: BTreeMap::new(),
            requests: BTreeMap::new(),
        }
    }
}

/// A day as saved, in the shape of this version or of an earlier one. Books
/// saved before marks kept every slice; on loading, those slices are folded
/// into marks and dropped. Their counted seconds were kept beside them and
/// are taken as they are, so the book counts the same.
#[derive(Deserialize)]
struct SavedDay {
    day: i64,
    extra_secs: u64,
    #[serde(default)]
    slices: BTreeMap<String, Slice>,
    #[serde(default)]
    marks: BTreeMap<String, u64>,
    counted: BTreeMap<String, u64>,
    #[serde(default)]
    removed: BTreeMap<String, u64>,
    #[serde(default)]
    extras: BTreeMap<String, ExtraTime>,
    #[serde(default)]
    requests: BTreeMap<String, RequestRecord>,
}

impl From<SavedDay> for DayRecord {
    fn from(saved: SavedDay) -> DayRecord {
        let mut marks = saved.marks;
        for slice in saved.slices.values() {
            if let Some(number) = slice_number(&slice.device_id, &slice.id) {
                let mark = marks.entry(slice.device_id.clone()).or_insert(0);
                *mark = (*mark).max(number);
            }
        }
        DayRecord {
            day: saved.day,
            extra_secs: saved.extra_secs,
            marks,
            counted: saved.counted,
            removed: saved.removed,
            extras: saved.extras,
            requests: saved.requests,
        }
    }
}

/// The number in a slice id made by `device_id`: the id is the device id, a
/// slash and the number. None for an id of any other shape, which no device
/// makes.
fn slice_number(device_id: &str, slice_id: &str) -> Option<u64> {
    let digits = slice_id.strip_prefix(device_id)?.strip_prefix('/')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// A request and its answer, as the master keeps them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RequestRecord {
    request: TimeRequest,
    answer: RequestAnswer,
    answered_at: Option<i64>,
}

impl RequestRecord {
    fn view(&self) -> RequestView {
        RequestView {
            request: self.request.clone(),
            answer: self.answer,
            answered_at: self.answered_at,
            reached_master: true,
        }
    }
}

/// The key a piece of extra time is kept under.
fn extra_key(source: &ExtraSource, id: &str) -> String {
    let kind = match source {
        ExtraSource::Guardian => "given",
        ExtraSource::Request { .. } => "answer",
        ExtraSource::Code { .. } | ExtraSource::Pin { .. } => "added",
    };
    format!("{kind}:{id}")
}

/// What a device may still spend on one day, worked out by the master.
struct Exposure {
    /// The device may hold rules with no limit that day.
    unbounded: bool,
    lease_total_secs: u64,
    lease_used_secs: u64,
    away_cap_secs: u64,
    away_used_secs: u64,
    /// Time added on the device that day, as far as the master knows.
    added_secs: u64,
    added_used_secs: u64,
}

impl Exposure {
    /// Away time held and not yet used.
    fn away_left(&self) -> u64 {
        if self.unbounded { 0 } else { self.away_cap_secs.saturating_sub(self.away_used_secs) }
    }

    fn remaining(&self) -> Option<u64> {
        if self.unbounded {
            return None;
        }
        Some(
            self.lease_total_secs
                .saturating_sub(self.lease_used_secs)
                .saturating_add(self.away_cap_secs.saturating_sub(self.away_used_secs))
                .saturating_add(self.added_secs.saturating_sub(self.added_used_secs)),
        )
    }
}

/// Seconds as an `i64`, without wrapping: figures from a faulty device can
/// be any size, and must never turn negative and widen the pool.
fn secs_i64(secs: u64) -> i64 {
    i64::try_from(secs).unwrap_or(i64::MAX)
}

/// Sums seconds without overflowing.
fn total(secs: impl Iterator<Item = u64>) -> u64 {
    secs.fold(0, u64::saturating_add)
}

/// What can still be handed out for a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Room {
    /// The day has no limit.
    NoLimit,
    /// Budget plus extra less use and reservations; below 0 after a cut.
    Secs(i64),
    /// A device may hold rules with no limit for the day, so nothing can be
    /// handed out.
    Nothing,
}

impl Room {
    fn at_most(self, wanted: u64) -> u64 {
        match self {
            Room::NoLimit => wanted,
            Room::Secs(s) => wanted.min(s.max(0) as u64),
            Room::Nothing => 0,
        }
    }

    fn take(&mut self, secs: u64) {
        if let Room::Secs(s) = self {
            *s = s.saturating_sub(secs_i64(secs));
        }
    }

    fn free(self) -> Option<u64> {
        match self {
            Room::NoLimit => None,
            Room::Secs(s) => Some(s.max(0) as u64),
            Room::Nothing => Some(0),
        }
    }
}

/// Everything the master keeps for one child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterBook {
    child_id: String,
    /// The rules, oldest first; the last is current. Older versions are kept
    /// while a device may still hold them.
    rules: Vec<RuleSet>,
    devices: Vec<DeviceRecord>,
    days: Vec<DayRecord>,
    next_grant: u64,
    forgotten_before: Option<i64>,
    /// The latest time the master has been given, unix seconds.
    #[serde(default)]
    clock: i64,
    /// The guardian's lock or unlock for all of the child's devices.
    #[serde(default)]
    guardian: GuardianSetting,
    /// The version of the PIN, 0 when never set.
    #[serde(default)]
    pin_version: u32,
    /// The PIN record, or None when there is no PIN.
    #[serde(default)]
    pin: Option<PinRecord>,
    /// Grows whenever the lock or unlock, the PIN or an answer to a request
    /// changes, so that devices that have not taken the change can be asked
    /// to report.
    #[serde(default)]
    news: u64,
    /// Numbers the extra time given without an id.
    #[serde(default)]
    last_unnamed_extra: u64,
    /// Devices removed from this child, by id, with when they were removed.
    /// Kept for good: anything that arrives later from one of them is
    /// refused, and its id is never paired again.
    #[serde(default)]
    removed: BTreeMap<String, i64>,
}

/// What a report hands over that the master now has.
#[derive(Debug, Clone, Default)]
struct Confirmed {
    slices: Vec<String>,
    added: Vec<String>,
    requests: Vec<String>,
}

impl MasterBook {
    /// A new book for a child, with its first rules.
    pub fn new(child_id: &str, rules: RuleSet) -> Result<MasterBook, MasterError> {
        let problems = rules.validate();
        if !problems.is_empty() {
            return Err(MasterError::InvalidRules { problems });
        }
        Ok(MasterBook {
            child_id: child_id.to_string(),
            rules: vec![rules],
            devices: Vec::new(),
            days: Vec::new(),
            next_grant: 1,
            forgotten_before: None,
            clock: i64::MIN,
            guardian: GuardianSetting::default(),
            pin_version: 0,
            pin: None,
            news: 0,
            last_unnamed_extra: 0,
            removed: BTreeMap::new(),
        })
    }

    /// The child.
    pub fn child_id(&self) -> String {
        self.child_id.clone()
    }

    /// The current rules.
    pub fn rules(&self) -> RuleSet {
        self.current_rules().clone()
    }

    /// The rules with this version, if still kept: a version is kept while a
    /// device may still hold it.
    pub fn rules_version(&self, version: u32) -> Option<RuleSet> {
        self.kept_rules(version).cloned()
    }

    fn current_rules(&self) -> &RuleSet {
        self.rules.last().expect("a book always has rules")
    }

    fn kept_rules(&self, version: u32) -> Option<&RuleSet> {
        self.rules.iter().find(|r| r.version == version)
    }

    /// The home day of `now` under the current rules.
    pub fn today(&self, now: i64) -> i64 {
        self.current_rules().calendar().day_of(now)
    }

    /// Takes new rules, with a version above the current one. They reach
    /// each device with its next grant, and until a device reports holding
    /// them it is in [`MasterBook::wanted_reports`]. Prefer
    /// [`MasterBook::change_rules`], which sets the version itself.
    pub fn set_rules(&mut self, rules: RuleSet) -> Result<(), MasterError> {
        let problems = rules.validate();
        if !problems.is_empty() {
            return Err(MasterError::InvalidRules { problems });
        }
        let current = self.current_rules().version;
        if rules.version <= current {
            return Err(MasterError::RulesNotNewer { current, given: rules.version });
        }
        self.rules.push(rules);
        // New rules are news: every device is asked to report until it has
        // taken them.
        self.news += 1;
        self.forget_unused_rules();
        Ok(())
    }

    /// Takes new rules as the next version: one above the current one,
    /// whatever `rules.version` says. Returns the rules as taken. They reach
    /// each device with its next grant, and until a device reports holding
    /// them it is in [`MasterBook::wanted_reports`].
    pub fn change_rules(&mut self, rules: RuleSet) -> Result<RuleSet, MasterError> {
        let current = self.current_rules().version;
        let version = current.checked_add(1).ok_or(MasterError::RulesNotNewer { current, given: current })?;
        let mut rules = rules;
        rules.version = version;
        self.set_rules(rules.clone())?;
        Ok(rules)
    }

    /// Pairs a device and returns its first grant, from which the device
    /// makes its state ([`crate::device::DeviceState::from_pairing`]). An id
    /// that was removed is never paired again.
    pub fn add_device(&mut self, device_id: &str, kind: DeviceKind, now: i64) -> Result<Grant, MasterError> {
        if self.removed.contains_key(device_id) {
            return Err(MasterError::DeviceRemoved { device_id: device_id.to_string() });
        }
        if self.devices.iter().any(|d| d.device_id == device_id) {
            return Err(MasterError::DeviceAlreadyPaired { device_id: device_id.to_string() });
        }
        self.clock = self.clock.max(now);
        self.devices.push(DeviceRecord {
            device_id: device_id.to_string(),
            kind,
            last_report: None,
            unacknowledged: Vec::new(),
            last_seen: Some(now),
            need: None,
            code_secret: Secret::default(),
            paired_rules: self.current_rules().version,
            paired_news: self.news,
        });
        let index = self.devices.len() - 1;
        let day = self.today(now);
        Ok(self.make_grant(index, day, Ask::default(), now, Confirmed::default()))
    }

    /// The ids of the paired devices, in the order they were paired. Removed
    /// devices are not among them.
    pub fn device_ids(&self) -> Vec<String> {
        self.devices.iter().map(|d| d.device_id.clone()).collect()
    }

    /// The ids of the devices removed from this child.
    pub fn removed_device_ids(&self) -> Vec<String> {
        self.removed.keys().cloned().collect()
    }

    /// Removes a device from this child.
    ///
    /// - Its counted use stays counted, day by day, on every day the book
    ///   keeps, so removing a device never gives its used time back to the
    ///   child ([`MasterBook::counted_use`] shows it as removed).
    /// - It holds nothing from now on: no lease, no away allowance and no
    ///   away share. What was held back for it is free for the other devices
    ///   at once, and the other phones can be given its away share.
    /// - Time added on it by a code or the PIN that it had not used, as far
    ///   as its newest report said, is dropped from its day's extra time
    ///   ([`ExtraTime::dropped_secs`]): it belonged to that device, and no
    ///   other device may spend it. The part it used stays counted.
    /// - Its record goes: grants it has not acknowledged, an unmet need, and
    ///   its code secret. Anything that arrives from it later is refused with
    ///   [`MasterError::DeviceRemoved`], and its id is never paired again.
    ///
    /// Use it had not yet reported is not counted, and the master no longer
    /// holds anything back for it. So remove a device after its last report
    /// when it can still make one, or when it is gone for good (lost, broken,
    /// reset); a removed device that is still used beside the others can pass
    /// the day's budget by what it holds. See the crate's README.
    ///
    /// Removing a device that is not paired is an error, and so is removing
    /// one twice.
    pub fn remove_device(&mut self, device_id: &str, now: i64) -> Result<(), MasterError> {
        let index = self.device_index(device_id)?;
        self.clock = self.clock.max(now);
        let device = self.devices.remove(index);
        let forgotten = |day: i64| self.forgotten_before.is_some_and(|f| day < f);
        let mut days: Vec<i64> = self.days.iter().map(|d| d.day).collect();
        if let Some(report) = &device.last_report
            && !forgotten(report.day)
            && !days.contains(&report.day)
        {
            days.push(report.day);
        }
        for day in days {
            let counted = self.counted_secs(&device, day);
            if counted == 0 && self.day_record(day).is_none() {
                continue;
            }
            let record = self.day_record_mut(day);
            record.counted.remove(device_id);
            record.marks.remove(device_id);
            if counted > 0 {
                record.removed.insert(device_id.to_string(), counted);
            }
        }
        if let Some(report) = &device.last_report {
            self.drop_unused_added(device_id, report.day, report.added_used_secs);
        }
        self.removed.insert(device_id.to_string(), now);
        self.forget_unused_rules();
        Ok(())
    }

    /// Drops from `day`'s extra time what a removed device added on itself
    /// and did not use: the pieces it added last are dropped first, as the
    /// first ones were spent first.
    fn drop_unused_added(&mut self, device_id: &str, day: i64, used: u64) {
        let Some(record) = self.days.iter_mut().find(|d| d.day == day) else { return };
        let mut pieces: Vec<&mut ExtraTime> = record
            .extras
            .values_mut()
            .filter(|e| matches!(&e.source, ExtraSource::Code { device_id: d, .. } | ExtraSource::Pin { device_id: d } if d == device_id))
            .collect();
        let added = total(pieces.iter().map(|e| e.secs.saturating_sub(e.dropped_secs)));
        let mut unused = added.saturating_sub(used);
        pieces.sort_by(|a, b| (b.at, &b.id).cmp(&(a.at, &a.id)));
        for piece in pieces {
            if unused == 0 {
                break;
            }
            let drop = unused.min(piece.secs.saturating_sub(piece.dropped_secs));
            piece.dropped_secs = piece.dropped_secs.saturating_add(drop);
            unused -= drop;
        }
    }

    /// Where a live device is in the list, or why it is not there.
    fn device_index(&self, device_id: &str) -> Result<usize, MasterError> {
        if let Some(index) = self.devices.iter().position(|d| d.device_id == device_id) {
            return Ok(index);
        }
        if self.removed.contains_key(device_id) {
            return Err(MasterError::DeviceRemoved { device_id: device_id.to_string() });
        }
        Err(MasterError::UnknownDevice { device_id: device_id.to_string() })
    }

    /// Handles a report from a device: returns the grant to send back, with
    /// what the book took from the report for the first time
    /// ([`ReportOutcome`]).
    ///
    /// Slices are counted once each, whatever the number of times they come;
    /// so are pieces of time added on the device (they join that day's extra
    /// time) and requests for more time. The figures of the report are kept
    /// if it is the newest from that device. The grant tops the lease up to
    /// one lease length when the device asked and the pool allows, and
    /// carries the device's away allowance and the current rules. What the
    /// pool cannot give is kept as the device's unmet need, and the away
    /// allowances of phones that report are lowered to meet it (see
    /// [`MasterBook::wanted_reports`]).
    ///
    /// **How a slice counts once.** A device numbers its slices 1, 2, 3 and
    /// on, never reusing a number, so that the slices of one home day carry
    /// numbers in one unbroken run; every report carries every slice the
    /// master has not confirmed; and a device drops a slice only once a
    /// grant confirmed it, that is after the master took it. The book keeps,
    /// for each device and day, the highest number it has taken (its mark),
    /// and takes a slice of that day when its number is above the mark. That
    /// counts every slice exactly once, through lost, repeated and late
    /// reports: when the mark is at n, the report that brought slice n
    /// carried every slice of the day before n that was not yet confirmed,
    /// and the confirmed ones had been taken already, so every number up to
    /// n has been taken. The slices of one report are taken together, so
    /// their order in it does not matter. A slice whose id is not the device
    /// id, a slash and a number is confirmed and not counted, like a slice
    /// that names another device: no device makes one.
    ///
    /// A device that loses its saved state and starts numbering at 1 again
    /// is not covered: on a day the book has a mark for it, its new slices
    /// are taken as already counted until its numbers pass the mark. Remove
    /// it and pair it again, under a new id (see the crate's README).
    ///
    /// A report from a removed device is refused with
    /// [`MasterError::DeviceRemoved`] and changes nothing.
    ///
    /// Save the book before sending the grant: the grant must be on record
    /// before the device can spend it.
    pub fn handle_report(&mut self, report: &Report, now: i64) -> Result<ReportOutcome, MasterError> {
        if report.child_id != self.child_id {
            return Err(MasterError::WrongChild {
                expected: self.child_id.clone(),
                given: report.child_id.clone(),
            });
        }
        let index = self.device_index(&report.device_id)?;

        self.clock = self.clock.max(now);
        let (slices, new_slices) = self.take_slices(&report.device_id, &report.slices);
        let (added, new_added) = self.take_added(&report.device_id, &report.added);
        let (requests, new_requests) = self.take_requests(&report.device_id, &report.requests);
        let confirmed = Confirmed { slices, added, requests };

        let device = &mut self.devices[index];
        device.last_seen = Some(device.last_seen.map_or(now, |seen| seen.max(now)));
        let (newest, same) = match &device.last_report {
            None => (true, false),
            Some(last) => (
                report.message_number > last.message_number,
                report.message_number == last.message_number,
            ),
        };
        if newest {
            device.last_report = Some(LastReport::from_report(report, now));
            device.unacknowledged.retain(|g| g.number > report.grant_number);
        }
        let day = device.last_report.as_ref().map_or(report.day, |r| r.day);
        self.forget_unused_rules();

        // Only the newest report, or a repeat of it, may ask for more, and
        // only it says what the device still needs.
        let current = newest || same;
        let want_more = report.want_more && current;
        let ask = Ask { want_more, current, answers_message: report.message_number };
        let grant = self.make_grant(index, day, ask, now, confirmed);
        Ok(ReportOutcome { grant, new_slices, new_added, new_requests, newest })
    }

    /// The devices the master would like to hear from now, so that time can
    /// move to where it is needed. The hub should ask each one it can reach
    /// to report at once; without that, the time moves at their next regular
    /// report. They are:
    ///
    /// - phones holding away time the master would take back for a device
    ///   that is waiting;
    /// - phones that must acknowledge a lowered away allowance before the
    ///   time is free;
    /// - devices that are waiting, once time is free for them;
    /// - devices that have not reported holding the current rules, or the
    ///   guardian's newest lock or unlock, PIN or answer to a request. Their
    ///   next grant brings it, and the report after it says so.
    ///
    /// The book cannot know which devices are in reach, so it names them all;
    /// asking one that cannot be reached costs nothing.
    pub fn wanted_reports(&self, now: i64) -> Vec<String> {
        let day = self.today(now);
        let clock = self.clock.max(now);
        let needs = total(self.devices.iter().filter_map(|d| d.need_on(day, clock)).map(|n| n.secs));
        let releasing = total(self.devices.iter().map(|d| self.pending_release(d, day, Some(clock))));
        // What the needs still lack once lowered allowances are acknowledged
        // and free time is handed out.
        let uncovered = match self.room(day).free() {
            None => 0,
            Some(free) => needs.saturating_sub(releasing).saturating_sub(free),
        };
        let mut wanted = std::collections::BTreeSet::new();
        for (i, device) in self.devices.iter().enumerate() {
            let pending = self.pending_release(device, day, None);
            if pending > 0 {
                wanted.insert(device.device_id.clone());
            }
            let can_give = device.kind == DeviceKind::Portable
                && device.need_on(day, clock).is_none()
                && pending == 0
                && self.exposure(device, day).away_left() > 0;
            if can_give && uncovered > 0 {
                wanted.insert(device.device_id.clone());
            }
            if let Some(need) = device.need_on(day, clock) {
                let mut free = self.room(day);
                free.take(self.older_needs(i, day, Some(need.since), clock));
                if free.free().is_none_or(|f| f > 0) {
                    wanted.insert(device.device_id.clone());
                }
            }
            if !self.has_latest_news(device) {
                wanted.insert(device.device_id.clone());
            }
        }
        wanted.into_iter().collect()
    }

    /// True when a device has reported holding the current rules and the
    /// newest guardian news.
    fn has_latest_news(&self, device: &DeviceRecord) -> bool {
        device.news_held() >= self.news && device.rules_held() >= self.current_rules().version
    }

    /// Time devices asked for on `day` and could not be given yet (needs
    /// asked for within [`NEED_KEPT_SECS`] of the latest report).
    pub fn unmet_need_secs(&self, day: i64) -> u64 {
        total(self.devices.iter().filter_map(|d| d.need_on(day, self.clock)).map(|n| n.secs))
    }

    /// Adds extra time to a day's pool without an id of the caller's: it is
    /// recorded under a new id each time, so calling it twice gives twice.
    /// Prefer [`MasterBook::give_extra`], which counts an id once.
    pub fn add_extra_secs(&mut self, day: i64, secs: u64) {
        if secs == 0 {
            return;
        }
        self.last_unnamed_extra += 1;
        let id = format!("unnamed/{}", self.last_unnamed_extra);
        let at = self.clock;
        self.record_extra(ExtraTime { id, day, secs, source: ExtraSource::Guardian, at, dropped_secs: 0 });
    }

    /// A guardian gives `minutes` of extra time for today, 1 to 1440. The
    /// same `id` is counted once: true when it was given now, false when it
    /// had been given already (and nothing changed). The time goes into the
    /// day's pool, and devices in reach get it with their next leases.
    pub fn give_extra(&mut self, id: &str, minutes: u32, now: i64) -> Result<bool, MasterError> {
        if minutes == 0 || minutes > MAX_EXTRA_MINUTES {
            return Err(MasterError::MinutesOutOfRange { minutes });
        }
        self.clock = self.clock.max(now);
        let day = self.today(now);
        let extra = ExtraTime {
            id: id.to_string(),
            day,
            secs: minutes as u64 * 60,
            source: ExtraSource::Guardian,
            at: now,
            dropped_secs: 0,
        };
        Ok(self.record_extra(extra))
    }

    /// Records a piece of extra time once, by where it came from and its id.
    /// True when it is new.
    fn record_extra(&mut self, extra: ExtraTime) -> bool {
        let key = extra_key(&extra.source, &extra.id);
        let record = self.day_record_mut(extra.day);
        if record.extras.contains_key(&key) {
            return false;
        }
        record.extras.insert(key, extra);
        true
    }

    /// Every piece of a day's extra time, oldest first: where the day's extra
    /// came from. Extra time a book saved before pieces had ids is shown as
    /// one piece with the id "earlier".
    pub fn extra_time(&self, day: i64) -> Vec<ExtraTime> {
        let Some(record) = self.day_record(day) else { return Vec::new() };
        let mut pieces: Vec<ExtraTime> = record.extras.values().cloned().collect();
        pieces.sort_by(|a, b| (a.at, &a.id).cmp(&(b.at, &b.id)));
        if record.extra_secs > 0 {
            pieces.insert(
                0,
                ExtraTime {
                    id: "earlier".into(),
                    day,
                    secs: record.extra_secs,
                    source: ExtraSource::Guardian,
                    at: i64::MIN,
                    dropped_secs: 0,
                },
            );
        }
        pieces
    }

    /// Every request for more time made for `day`, with its answer, oldest
    /// first.
    pub fn requests(&self, day: i64) -> Vec<RequestView> {
        let Some(record) = self.day_record(day) else { return Vec::new() };
        let mut views: Vec<RequestView> = record.requests.values().map(RequestRecord::view).collect();
        views.sort_by(|a, b| (a.request.asked_at, &a.request.id).cmp(&(b.request.asked_at, &b.request.id)));
        views
    }

    /// The requests for today still waiting for an answer, oldest first.
    pub fn waiting_requests(&self, now: i64) -> Vec<RequestView> {
        let mut views = self.requests(self.today(now));
        views.retain(|v| v.answer == RequestAnswer::Waiting);
        views
    }

    /// A guardian answers a request: grants it with a number of minutes
    /// (which may differ from those asked for), or refuses it. A request is
    /// answered once, and only while its day is not over. Granted minutes go
    /// into that day's pool as extra time, once. The answer reaches every one
    /// of the child's devices with its next grant.
    pub fn answer_request(
        &mut self,
        request_id: &str,
        reply: RequestReply,
        now: i64,
    ) -> Result<RequestView, MasterError> {
        if let RequestReply::Grant { minutes } = reply
            && (minutes == 0 || minutes > MAX_EXTRA_MINUTES)
        {
            return Err(MasterError::MinutesOutOfRange { minutes });
        }
        let unknown = || MasterError::UnknownRequest { request_id: request_id.to_string() };
        let day = self
            .days
            .iter()
            .find(|d| d.requests.contains_key(request_id))
            .map(|d| d.day)
            .ok_or_else(unknown)?;
        self.clock = self.clock.max(now);
        let today = self.today(now);
        let record = self.day_record_mut(day).requests.get_mut(request_id).ok_or_else(unknown)?;
        if record.answer != RequestAnswer::Waiting {
            return Err(MasterError::AlreadyAnswered { request_id: request_id.to_string() });
        }
        if record.request.day < today {
            return Err(MasterError::RequestTooLate { request_id: request_id.to_string() });
        }
        record.answer = match reply {
            RequestReply::Grant { minutes } => RequestAnswer::Granted { minutes },
            RequestReply::Refuse => RequestAnswer::Refused,
        };
        record.answered_at = Some(now);
        let view = record.view();
        if let RequestReply::Grant { minutes } = reply {
            self.record_extra(ExtraTime {
                id: request_id.to_string(),
                day,
                secs: minutes as u64 * 60,
                source: ExtraSource::Request { request_id: request_id.to_string() },
                at: now,
                dropped_secs: 0,
            });
        }
        self.news += 1;
        Ok(view)
    }

    /// Keeps a device's code secret: the 32 bytes made by the caller at
    /// pairing and also given to the device
    /// ([`crate::device::DeviceState::set_code_secret`]).
    pub fn set_code_secret(&mut self, device_id: &str, secret: Vec<u8>) -> Result<(), MasterError> {
        check_secret(&secret).map_err(|_| MasterError::SecretWrongLength {
            given: u32::try_from(secret.len()).unwrap_or(u32::MAX),
        })?;
        let index = self.device_index(device_id)?;
        self.devices[index].code_secret = Secret::new(secret);
        Ok(())
    }

    /// A device's code secret, to hand to a guardian phone at home so that
    /// it can make one time codes ([`crate::codes::next_code`]); None when
    /// none was set. Never show it or log it. A removed device's secret is
    /// gone with it.
    pub fn code_secret(&self, device_id: &str) -> Result<Option<Vec<u8>>, MasterError> {
        let device = &self.devices[self.device_index(device_id)?];
        Ok(device.code_secret.is_set().then(|| device.code_secret.bytes().to_vec()))
    }

    /// Sets the guardian PIN, 6 to 8 digits, with a salt of 16 bytes or more
    /// made by the caller. Hashing it is slow on purpose. It reaches every
    /// device with its next grant, and replaces the old PIN there. Returns
    /// the new version.
    pub fn set_pin(&mut self, pin: &str, salt: Vec<u8>) -> Result<u32, MasterError> {
        let record = PinRecord::new(pin, salt).map_err(|problem| MasterError::BadPin { problem })?;
        self.set_pin_record(record)
    }

    /// Sets a PIN record made elsewhere, such as on a guardian phone, or
    /// with fewer rounds in tests. It must be sound
    /// ([`PinRecord::is_sound`]). A record does not say how long its PIN is,
    /// so the maker must make it from 6 to 8 digits, as [`PinRecord::new`]
    /// does. Returns the new version.
    pub fn set_pin_record(&mut self, record: PinRecord) -> Result<u32, MasterError> {
        if !record.is_sound() {
            return Err(MasterError::BadPin { problem: PinError::NotSound });
        }
        self.pin = Some(record);
        self.pin_version += 1;
        self.news += 1;
        Ok(self.pin_version)
    }

    /// Removes the PIN. Devices then refuse PIN actions. Returns the new
    /// version.
    pub fn clear_pin(&mut self) -> u32 {
        self.pin = None;
        self.pin_version += 1;
        self.news += 1;
        self.pin_version
    }

    /// The version of the PIN, 0 when never set.
    pub fn pin_version(&self) -> u32 {
        self.pin_version
    }

    /// True when a PIN is set.
    pub fn has_pin(&self) -> bool {
        self.pin.is_some()
    }

    /// Locks all of the child's devices now, apart from always allowed apps,
    /// until `until`, or until lifted when None. It reaches each device with
    /// its next grant; ask the devices in [`MasterBook::wanted_reports`] to
    /// report at once.
    pub fn lock_now(&mut self, until: Option<i64>, now: i64) -> Result<GuardianSetting, MasterError> {
        if until.is_some_and(|u| u <= now) {
            return Err(MasterError::TimeOutOfRange);
        }
        Ok(self.set_guardian(GuardianMode::Lock { until }, now))
    }

    /// Unlocks all of the child's devices until `until`, at most a day
    /// ahead: blocked hours and a lack of time are lifted, blocked apps stay
    /// blocked, and time used still counts, so the day's budget may run out
    /// and stay out.
    pub fn unlock_until(&mut self, until: i64, now: i64) -> Result<GuardianSetting, MasterError> {
        if until <= now || until > now.saturating_add(MAX_UNLOCK_SECS) {
            return Err(MasterError::TimeOutOfRange);
        }
        Ok(self.set_guardian(GuardianMode::Unlock { until }, now))
    }

    /// Lifts the guardian's lock or unlock, and any unlock set with the PIN
    /// on a device before now. The rules alone decide again.
    pub fn lift(&mut self, now: i64) -> GuardianSetting {
        self.set_guardian(GuardianMode::Nothing, now)
    }

    fn set_guardian(&mut self, mode: GuardianMode, now: i64) -> GuardianSetting {
        self.clock = self.clock.max(now);
        self.guardian = GuardianSetting { version: self.guardian.version.saturating_add(1), mode, set_at: now };
        self.news += 1;
        self.guardian
    }

    /// The guardian's lock or unlock as last set.
    pub fn guardian_setting(&self) -> GuardianSetting {
        self.guardian
    }

    /// One day's pool as it stands.
    pub fn pool(&self, day: i64) -> PoolView {
        let budget = self.current_rules().budget_secs_on(day);
        let extra = self.extra_secs(day);
        let used = self.used_secs(day);
        let reserved = self.reserved_secs(day);
        let room = self.room(day);
        let over = match (budget, reserved) {
            (Some(b), Some(r)) => used.saturating_add(r).saturating_sub(b.saturating_add(extra)),
            _ => 0,
        };
        PoolView {
            day,
            budget_secs: budget,
            extra_secs: extra,
            used_secs: used,
            reserved_secs: reserved,
            free_secs: room.free(),
            over_secs: over,
        }
    }

    /// One paired device's part of a day as it stands, and what the master
    /// knows of it.
    pub fn device_view(&self, device_id: &str, day: i64) -> Result<DeviceView, MasterError> {
        let device = &self.devices[self.device_index(device_id)?];
        let exposure = self.exposure(device, day);
        Ok(DeviceView {
            device_id: device.device_id.clone(),
            kind: device.kind,
            used_secs: self.counted_secs(device, day),
            reserved_secs: exposure.remaining(),
            giving_back_secs: self.pending_release(device, day, Some(self.clock)),
            last_seen: device.last_seen,
            waiting_secs: device.need_on(day, self.clock).map_or(0, |n| n.secs),
            added_secs: exposure.added_secs.max(self.added_on_device(&device.device_id, day)),
            wrong_guesses: device.last_report.as_ref().map_or(0, |r| r.wrong_guesses),
            pin_unlocked_until: device
                .last_report
                .as_ref()
                .and_then(|r| r.pin_unlock)
                .map(|p| p.until)
                .filter(|until| *until > self.clock),
            rules_version: device.rules_held(),
            has_latest_news: self.has_latest_news(device),
            clock_off_by_secs: device.last_report.as_ref().and_then(|r| r.clock_off_by),
        })
    }

    /// What the master would say is left for a device that day: what it may
    /// still spend from what it holds, plus what the master could still hand
    /// it (free time that is not waiting for an older need of another
    /// device). None when there is no limit.
    pub fn time_left_for(&self, device_id: &str, day: i64) -> Result<Option<u64>, MasterError> {
        let index = self.device_index(device_id)?;
        let device = &self.devices[index];
        let reserved = self.exposure(device, day).remaining();
        let mut free = self.room(day);
        let since = device.need_on(day, self.clock).map(|n| n.since);
        free.take(self.older_needs(index, day, since, self.clock));
        Ok(match (free.free(), reserved) {
            (Some(free), Some(held)) => Some(free.saturating_add(held)),
            _ => None,
        })
    }

    /// Each device's counted use on `day` as the book keeps it: the paired
    /// devices in the order they were paired, then the removed ones that
    /// used time that day. The slices themselves are not kept by the book;
    /// a caller that shows them keeps the new slices of every
    /// [`ReportOutcome`].
    pub fn counted_use(&self, day: i64) -> Vec<CountedUse> {
        let mut uses: Vec<CountedUse> = self
            .devices
            .iter()
            .map(|d| CountedUse { device_id: d.device_id.clone(), counted_secs: self.counted_secs(d, day), removed: false })
            .collect();
        if let Some(record) = self.day_record(day) {
            uses.extend(record.removed.iter().map(|(id, secs)| CountedUse {
                device_id: id.clone(),
                counted_secs: *secs,
                removed: true,
            }));
        }
        uses
    }

    /// Drops the record of every day before `day`. Slices, added time and
    /// requests for those days that arrive later are confirmed but no longer
    /// taken. True when the book changed, so that it needs saving.
    pub fn forget_days_before(&mut self, day: i64) -> bool {
        let before = self.days.len();
        self.days.retain(|d| d.day >= day);
        let forgotten_before = Some(self.forgotten_before.map_or(day, |f| f.max(day)));
        let changed = self.days.len() != before || forgotten_before != self.forgotten_before;
        self.forgotten_before = forgotten_before;
        changed
    }

    // Inner workings.

    fn day_record_mut(&mut self, day: i64) -> &mut DayRecord {
        let at = match self.days.binary_search_by_key(&day, |d| d.day) {
            Ok(at) => at,
            Err(at) => {
                self.days.insert(at, DayRecord::new(day));
                at
            }
        };
        &mut self.days[at]
    }

    fn day_record(&self, day: i64) -> Option<&DayRecord> {
        self.days.binary_search_by_key(&day, |d| d.day).ok().map(|at| &self.days[at])
    }

    fn extra_secs(&self, day: i64) -> u64 {
        self.day_record(day).map_or(0, |d| {
            d.extra_secs.saturating_add(total(d.extras.values().map(|e| e.secs.saturating_sub(e.dropped_secs))))
        })
    }

    /// Counted use on a day: every paired device's, and that of devices
    /// since removed.
    fn used_secs(&self, day: i64) -> u64 {
        let removed = self.day_record(day).map_or(0, |d| total(d.removed.values().copied()));
        total(self.devices.iter().map(|d| self.counted_secs(d, day))).saturating_add(removed)
    }

    /// Takes the slices of a report: counts the new ones once each (see
    /// [`MasterBook::handle_report`]). Returns the ids of all those handed
    /// over, which the device may then forget, and the slices taken now.
    fn take_slices(&mut self, device_id: &str, slices: &[Slice]) -> (Vec<String>, Vec<Slice>) {
        let mut confirmed = Vec::with_capacity(slices.len());
        let mut new = Vec::new();
        // The numbers taken in this report, by day, and the highest of them:
        // the marks move only once the whole report is taken, so the order of
        // its slices does not matter.
        let mut taken: BTreeSet<(i64, u64)> = BTreeSet::new();
        let mut highest: BTreeMap<i64, u64> = BTreeMap::new();
        for slice in slices {
            confirmed.push(slice.id.clone());
            let forgotten = self.forgotten_before.is_some_and(|f| slice.day < f);
            let Some(number) = slice_number(device_id, &slice.id) else { continue };
            if slice.child_id != self.child_id
                || slice.device_id != device_id
                || slice.end <= slice.start
                || forgotten
            {
                continue;
            }
            let mark = self.day_record(slice.day).and_then(|d| d.marks.get(device_id).copied()).unwrap_or(0);
            if number <= mark || !taken.insert((slice.day, number)) {
                continue;
            }
            if slice.counts {
                let record = self.day_record_mut(slice.day);
                let counted = record.counted.entry(device_id.to_string()).or_insert(0);
                *counted = counted.saturating_add(slice.secs());
            }
            let high = highest.entry(slice.day).or_insert(0);
            *high = (*high).max(number);
            new.push(slice.clone());
        }
        for (day, high) in highest {
            let mark = self.day_record_mut(day).marks.entry(device_id.to_string()).or_insert(0);
            *mark = (*mark).max(high);
        }
        (confirmed, new)
    }

    /// Records new pieces of time added on a device as that day's extra time,
    /// once each. Returns the ids of all those handed over, and the pieces
    /// taken now. A piece that cannot be right (for another child or device,
    /// nothing, or more than a day) is confirmed but not counted as extra;
    /// the device's own figures still make the master hold it back, so
    /// nothing can widen the pool.
    fn take_added(&mut self, device_id: &str, added: &[AddedTime]) -> (Vec<String>, Vec<AddedTime>) {
        let mut confirmed = Vec::with_capacity(added.len());
        let mut new = Vec::new();
        for piece in added {
            confirmed.push(piece.id.clone());
            let forgotten = self.forgotten_before.is_some_and(|f| piece.day < f);
            if piece.child_id != self.child_id
                || piece.device_id != device_id
                || piece.secs == 0
                || piece.secs > SECONDS_PER_DAY as u64
                || forgotten
            {
                continue;
            }
            let source = match piece.by {
                AddedBy::Code { selector } => ExtraSource::Code { device_id: device_id.to_string(), selector },
                AddedBy::Pin => ExtraSource::Pin { device_id: device_id.to_string() },
            };
            let fresh = self.record_extra(ExtraTime {
                id: piece.id.clone(),
                day: piece.day,
                secs: piece.secs,
                source,
                at: piece.at,
                dropped_secs: 0,
            });
            if fresh {
                new.push(piece.clone());
            }
        }
        (confirmed, new)
    }

    /// Records new requests for more time, once each. Returns the ids of all
    /// those handed over, and the requests taken now. A device may have a few
    /// requests a day on record; more from a faulty device are confirmed and
    /// dropped.
    fn take_requests(&mut self, device_id: &str, requests: &[TimeRequest]) -> (Vec<String>, Vec<TimeRequest>) {
        let mut confirmed = Vec::with_capacity(requests.len());
        let mut new = Vec::new();
        for request in requests {
            confirmed.push(request.id.clone());
            let forgotten = self.forgotten_before.is_some_and(|f| request.day < f);
            if request.child_id != self.child_id || request.device_id != device_id || forgotten {
                continue;
            }
            let record = self.day_record_mut(request.day);
            if record.requests.contains_key(&request.id) {
                continue;
            }
            let from_device = record.requests.values().filter(|r| r.request.device_id == device_id).count();
            if from_device as u64 >= 2 * MAX_REQUESTS_PER_DAY as u64 {
                continue;
            }
            record.requests.insert(
                request.id.clone(),
                RequestRecord { request: request.clone(), answer: RequestAnswer::Waiting, answered_at: None },
            );
            new.push(request.clone());
        }
        (confirmed, new)
    }

    /// Counted use of a device on a day: from its slices, or from its newest
    /// report if that says more.
    fn counted_secs(&self, device: &DeviceRecord, day: i64) -> u64 {
        let from_slices = self
            .day_record(day)
            .and_then(|d| d.counted.get(&device.device_id).copied())
            .unwrap_or(0);
        let from_report = device.last_report.as_ref().filter(|r| r.day == day).map_or(0, |r| r.used_secs);
        from_slices.max(from_report)
    }

    /// What a device may still spend on `day` without telling the master.
    fn exposure(&self, device: &DeviceRecord, day: i64) -> Exposure {
        let none = Exposure {
            unbounded: false,
            lease_total_secs: 0,
            lease_used_secs: 0,
            away_cap_secs: 0,
            away_used_secs: 0,
            added_secs: 0,
            added_used_secs: 0,
        };
        let last = device.last_report.as_ref();
        if last.is_some_and(|r| r.day > day) {
            // The device has moved past that day; all its use of the day was
            // in that report.
            return none;
        }
        let terms = device.possible_terms();
        let unbounded = terms.iter().any(|(_, version)| {
            self.kept_rules(*version).is_none_or(|r| r.budget_secs_on(day).is_none())
        });
        if unbounded {
            return Exposure { unbounded: true, ..none };
        }
        let sent_for_day = device.unacknowledged.iter().filter(|g| g.day == day);
        let same_day = last.filter(|r| r.day == day);
        let lease_total = sent_for_day
            .clone()
            .map(|g| g.lease_total_secs)
            .chain(same_day.map(|r| r.lease_total_secs))
            .max()
            .unwrap_or(0);
        let sent_cap = sent_for_day.map(|g| g.away_cap_secs).max().unwrap_or(0);
        let (away_cap, away_used, lease_used) = match same_day {
            Some(r) => (sent_cap.max(r.away_cap_secs), r.away_used_secs, r.lease_used_secs()),
            None => {
                // The device has not reported from this day. It may have
                // started it on its own, with an away allowance worked out
                // from any rules and away share it may hold.
                let from_terms = terms
                    .iter()
                    .filter_map(|(percent, version)| {
                        self.kept_rules(*version).map(|r| r.away_cap_secs(*percent, day))
                    })
                    .max()
                    .unwrap_or(0);
                (sent_cap.max(from_terms), 0, 0)
            }
        };
        // Time added on the device: every piece the master has heard of, or
        // what the newest report said if that is more. The device spends it
        // only on its own day, and the report says how much is used.
        let added = self
            .added_on_device(&device.device_id, day)
            .max(same_day.map_or(0, |r| r.added_secs));
        Exposure {
            unbounded: false,
            lease_total_secs: lease_total,
            lease_used_secs: lease_used,
            away_cap_secs: away_cap,
            away_used_secs: away_used,
            added_secs: added,
            added_used_secs: same_day.map_or(0, |r| r.added_used_secs),
        }
    }

    /// Time added on a device on `day` by codes and the PIN, as far as the
    /// master has heard.
    fn added_on_device(&self, device_id: &str, day: i64) -> u64 {
        let Some(record) = self.day_record(day) else { return 0 };
        total(record.extras.values().filter_map(|e| match &e.source {
            ExtraSource::Code { device_id: d, .. } | ExtraSource::Pin { device_id: d } if d == device_id => {
                Some(e.secs.saturating_sub(e.dropped_secs))
            }
            _ => None,
        }))
    }

    /// Time reserved for all devices on a day, or None when a device could
    /// spend any amount.
    fn reserved_secs(&self, day: i64) -> Option<u64> {
        self.devices
            .iter()
            .try_fold(0u64, |sum, d| self.exposure(d, day).remaining().map(|r| sum.saturating_add(r)))
    }

    fn room(&self, day: i64) -> Room {
        let Some(budget) = self.current_rules().budget_secs_on(day) else {
            return Room::NoLimit;
        };
        let Some(reserved) = self.reserved_secs(day) else {
            return Room::Nothing;
        };
        let used = self.used_secs(day);
        Room::Secs(
            secs_i64(budget)
                .saturating_add(secs_i64(self.extra_secs(day)))
                .saturating_sub(secs_i64(used))
                .saturating_sub(secs_i64(reserved)),
        )
    }

    /// Away time a device gives back once it acknowledges its newest grant:
    /// what the master holds back for it now, less what that grant leaves it.
    /// With a `clock`, only if that grant is less than [`NEED_KEPT_SECS`] old.
    fn pending_release(&self, device: &DeviceRecord, day: i64, clock: Option<i64>) -> u64 {
        let Some(newest) = device.unacknowledged.iter().filter(|g| g.day == day).max_by_key(|g| g.number)
        else {
            return 0;
        };
        if clock.is_some_and(|c| newest.sent_at.saturating_add(NEED_KEPT_SECS) < c) {
            return 0;
        }
        let exposure = self.exposure(device, day);
        if exposure.unbounded {
            return 0;
        }
        let offered_left = newest.away_cap_secs.saturating_sub(exposure.away_used_secs);
        exposure.away_left().saturating_sub(offered_left)
    }

    /// The away share to give a device in a grant for `day`, at most
    /// `percent`. A device that has not yet reported from a later day up to
    /// the master's own (its report came late, its clock is behind, or the
    /// home zone moved) may start that day on its own with the share this
    /// grant gives it. On each such day, the allowance it would start with
    /// must fit what is held back for it already plus what is free; the share
    /// is lowered until it does. Later days are fresh, and the shares of all
    /// phones add up to at most the whole budget.
    fn percent_that_fits(&self, index: usize, rules: &RuleSet, percent: u8, day: i64, now: i64) -> u8 {
        let today = self.today(now);
        if today <= day {
            return percent;
        }
        if today - day > 8 {
            // A clock this far off gets no away share until it is put right.
            return 0;
        }
        let device = &self.devices[index];
        let mut fits = percent;
        for later in day + 1..=today {
            let exposure = self.exposure(device, later);
            if exposure.unbounded {
                continue;
            }
            let held = exposure.away_left();
            let most = match self.room(later) {
                Room::NoLimit => continue,
                Room::Secs(free) => secs_i64(held).saturating_add(free).max(0) as u64,
                Room::Nothing => held,
            };
            while fits > 0 && rules.away_cap_secs(fits, later) > most {
                fits -= 1;
            }
        }
        fits
    }

    /// The unmet needs of the devices other than `index` on `day`: all of
    /// them, or only those that arose before `since`.
    fn older_needs(&self, index: usize, day: i64, since: Option<u64>, clock: i64) -> u64 {
        total(
            self.devices
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .filter_map(|(_, d)| d.need_on(day, clock))
                .filter(|n| since.is_none_or(|s| n.since < s))
                .map(|n| n.secs),
        )
    }

    /// Makes, records and returns a grant for device `index` on `day`.
    fn make_grant(&mut self, index: usize, day: i64, ask: Ask, now: i64, confirmed: Confirmed) -> Grant {
        let Ask { want_more, current: update_need, answers_message } = ask;
        let rules = self.current_rules().clone();
        // A device whose day is far from the master's has a wrong clock; it
        // is given nothing new until the two agree.
        let near = (day - self.today(now)).abs() <= 1;
        let device = &self.devices[index];
        let exposure = self.exposure(device, day);
        let mut room = self.room(day);
        // Free time waiting for another device that asked first is not for
        // this one.
        let clock = self.clock;
        let own_since = device.need_on(day, clock).map(|n| n.since);
        let for_others = self.older_needs(index, day, own_since, clock);

        // The lease: topped up to one lease length beyond what is spent.
        let mut lease_total = exposure.lease_total_secs;
        let mut shortfall = 0;
        if want_more && near {
            let target = exposure.lease_used_secs.saturating_add(rules.lease_secs());
            if target > lease_total {
                let wanted = target - lease_total;
                let mut for_me = room;
                for_me.take(for_others);
                let add = for_me.at_most(wanted);
                lease_total = lease_total.saturating_add(add);
                room.take(add);
                shortfall = wanted - add;
            }
        }
        // What this device still needs is what the pool could not give and
        // its own away allowance cannot cover.
        let need = shortfall.saturating_sub(exposure.away_left());

        // The away allowance. Each portable device gets the rules' away
        // share, but never so much that the shares the devices may hold add
        // up to more than the whole budget.
        let (away_percent, away_cap) = match device.kind {
            DeviceKind::OnMaster => (0, 0),
            DeviceKind::Portable => {
                let others: u32 = self
                    .devices
                    .iter()
                    .enumerate()
                    .filter(|(i, d)| *i != index && d.kind == DeviceKind::Portable)
                    .map(|(_, d)| d.highest_possible_percent() as u32)
                    .sum();
                let percent = (rules.away_share_percent as u32).min(100u32.saturating_sub(others)) as u8;
                let percent = self.percent_that_fits(index, &rules, percent, day, now);
                let target = rules.away_cap_secs(percent, day);
                // The allowance is a level that includes away time already
                // used; what the pool must cover is the part not yet used.
                let used = exposure.away_used_secs;
                let left_now = exposure.away_left();
                // Time other devices are waiting for, less what phones that
                // were already lowered will give back when they acknowledge.
                let releasing = total(
                    self.devices
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != index)
                        .map(|(_, d)| self.pending_release(d, day, Some(clock))),
                );
                // A device that is itself waiting is not drained for others.
                let claim = if need > 0 {
                    0
                } else {
                    self.older_needs(index, day, None, clock).saturating_sub(releasing)
                };
                // Never more left than the pool can cover once those needs are
                // met: what is held back now, plus what is free, less the
                // claim. Less than now after a cut, or while a device waits;
                // never below what is already used.
                let most_left = match room {
                    Room::NoLimit => u64::MAX,
                    Room::Secs(free) => secs_i64(left_now)
                        .saturating_add(free)
                        .saturating_sub(secs_i64(claim))
                        .max(0) as u64,
                    Room::Nothing => left_now,
                };
                let most_left = if near { most_left } else { most_left.min(left_now) };
                let target_left = target.saturating_sub(used);
                let cap = if target_left <= most_left { target } else { used.saturating_add(most_left) };
                room.take(target_left.min(most_left).saturating_sub(left_now));
                (percent, cap)
            }
        };
        let acknowledge_now = away_cap.saturating_sub(exposure.away_used_secs) < exposure.away_left();

        let mut for_you = room;
        for_you.take(for_others);
        let left_for_you_secs = for_you.free().map(|free| {
            free.saturating_add(lease_total.saturating_sub(exposure.lease_used_secs))
                .saturating_add(away_cap.saturating_sub(exposure.away_used_secs))
        });
        // Away time other phones hold that could come here if they gave it
        // all back: none of it while the pool is overdrawn by more.
        let held_by_others = total(
            self.devices
                .iter()
                .enumerate()
                .filter(|(i, d)| *i != index && d.kind == DeviceKind::Portable)
                .map(|(_, d)| self.exposure(d, day).away_left()),
        );
        let held_elsewhere_secs = match for_you {
            Room::Secs(free) => held_by_others.saturating_sub(free.min(0).unsigned_abs()),
            Room::NoLimit | Room::Nothing => 0,
        };
        let used_secs_reported =
            device.last_report.as_ref().filter(|r| r.day == day).map_or(0, |r| r.used_secs);
        // The PIN record travels until the device reports holding its version.
        let pin = (device.last_report.as_ref().map_or(0, |r| r.pin_version) < self.pin_version)
            .then(|| PinUpdate { version: self.pin_version, record: self.pin.clone() });
        let requests = self.requests(day);

        let number = self.next_grant;
        self.next_grant += 1;
        let device = &mut self.devices[index];
        if update_need {
            device.need = (need > 0).then(|| Need {
                day,
                secs: need,
                since: device.need_on(day, clock).map_or(number, |old| old.since),
                at: now,
            });
        }
        device.unacknowledged.push(SentGrant {
            number,
            day,
            lease_total_secs: lease_total,
            away_cap_secs: away_cap,
            away_percent,
            rules_version: rules.version,
            sent_at: now,
        });
        Grant {
            child_id: self.child_id.clone(),
            device_id: device.device_id.clone(),
            number,
            answers_message,
            day,
            lease_total_secs: lease_total,
            away_cap_secs: away_cap,
            away_percent,
            left_for_you_secs,
            held_elsewhere_secs,
            used_secs_reported,
            acknowledge_now,
            rules,
            confirmed_slices: confirmed.slices,
            confirmed_added: confirmed.added,
            confirmed_requests: confirmed.requests,
            guardian: self.guardian,
            pin,
            requests,
            news: self.news,
        }
    }

    /// Drops rule versions that no device can hold any more.
    fn forget_unused_rules(&mut self) {
        let oldest_in_use = self
            .devices
            .iter()
            .flat_map(|d| d.possible_terms().into_iter().map(|(_, v)| v))
            .chain(std::iter::once(self.current_rules().version))
            .min()
            .unwrap_or(0);
        self.rules.retain(|r| r.version >= oldest_in_use);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DeviceState;
    use crate::rules::DailyBudget;
    use crate::time::CivilDate;

    const MIN: u64 = 60;

    fn start() -> i64 {
        RuleSet::new(1, 19_800).calendar().at(CivilDate::new(2026, 11, 18).unwrap(), 8, 0, 0)
    }

    fn book() -> MasterBook {
        let mut rules = RuleSet::new(1, 19_800);
        rules.budgets = crate::rules::WeekBudgets::every_day(DailyBudget::Minutes(120));
        MasterBook::new("asha", rules).unwrap()
    }

    #[test]
    fn a_phone_is_given_its_away_share_at_pairing_and_it_is_reserved() {
        let mut b = book();
        let now = start();
        let welcome = b.add_device("phone", DeviceKind::Portable, now).unwrap();
        assert_eq!(welcome.away_cap_secs, 60 * MIN);
        assert_eq!(welcome.away_percent, 50);
        let pool = b.pool(b.today(now));
        assert_eq!(pool.reserved_secs, Some(60 * MIN));
        assert_eq!(pool.free_secs, Some(60 * MIN));
        let pc = b.add_device("pc", DeviceKind::OnMaster, now).unwrap();
        assert_eq!((pc.away_cap_secs, pc.away_percent), (0, 0));
    }

    #[test]
    fn a_slice_handed_over_twice_counts_once() {
        let mut b = book();
        let now = start();
        let mut pc = DeviceState::from_pairing(&b.add_device("pc", DeviceKind::OnMaster, now).unwrap(), now);
        let grant = b.handle_report(&pc.report(now, true), now).unwrap().grant;
        pc.take_grant(&grant, now);
        pc.record_use("game", now, now + 300);
        let report = pc.report(now + 300, false);
        assert_eq!(b.handle_report(&report, now + 300).unwrap().new_slices, report.slices);
        assert!(b.handle_report(&report, now + 301).unwrap().new_slices.is_empty());
        let mut again = report.clone();
        again.message_number += 7;
        let outcome = b.handle_report(&again, now + 302).unwrap();
        assert!(outcome.new_slices.is_empty() && outcome.newest);
        assert_eq!(b.pool(b.today(now)).used_secs, 300);
        let counted = b.counted_use(b.today(now));
        assert_eq!(counted, vec![CountedUse { device_id: "pc".into(), counted_secs: 300, removed: false }]);
    }

    #[test]
    fn slice_numbers_are_read_only_from_ids_a_device_makes() {
        assert_eq!(slice_number("phone", "phone/17"), Some(17));
        assert_eq!(slice_number("device-ab/c", "device-ab/c/3"), Some(3));
        for odd in ["phone", "phone/", "phone/x", "phone/+3", "phone/ 3", "phone/3/4", "pc/3", "phone2/3", "/3"] {
            assert_eq!(slice_number("phone", odd), None, "{odd}");
        }
        assert_eq!(slice_number("phone", "phone/99999999999999999999999"), None);
    }

    #[test]
    fn a_lost_grant_is_absorbed_by_the_next_one() {
        let mut b = book();
        let now = start();
        let mut pc = DeviceState::from_pairing(&b.add_device("pc", DeviceKind::OnMaster, now).unwrap(), now);
        let lost = b.handle_report(&pc.report(now, true), now).unwrap().grant;
        assert_eq!(lost.lease_total_secs, 10 * MIN);
        // The master keeps the lost lease reserved...
        assert_eq!(b.pool(b.today(now)).reserved_secs, Some(10 * MIN));
        // ...and the next grant is a level, not a second lease on top.
        let next = b.handle_report(&pc.report(now + 5, true), now + 5).unwrap().grant;
        assert_eq!(next.lease_total_secs, 10 * MIN);
        pc.take_grant(&next, now + 5);
        assert_eq!(pc.take_grant(&lost, now + 6), crate::device::GrantOutcome::Stale);
        b.handle_report(&pc.report(now + 7, false), now + 7).unwrap();
        assert_eq!(b.pool(b.today(now)).reserved_secs, Some(10 * MIN));
    }

    #[test]
    fn absurd_figures_from_a_faulty_device_neither_crash_nor_widen_the_pool() {
        let mut b = book();
        let now = start();
        let today = b.today(now);
        b.add_device("phone", DeviceKind::Portable, now).unwrap();
        b.add_device("pc", DeviceKind::OnMaster, now).unwrap();
        let free_before = b.pool(today).free_secs.unwrap();
        let slice = |id: &str, start: i64, end: i64| Slice {
            id: id.into(),
            child_id: "asha".into(),
            device_id: "phone".into(),
            app_id: "game".into(),
            day: today,
            start,
            end,
            counts: true,
        };
        for (rules_version, grant_number) in [(1, 1), (u32::MAX, u64::MAX)] {
            let report = Report {
                child_id: "asha".into(),
                device_id: "phone".into(),
                message_number: u64::MAX,
                sent_at: i64::MAX,
                rules_version,
                grant_number,
                away_percent: u8::MAX,
                day: today,
                used_secs: u64::MAX,
                lease_total_secs: u64::MAX,
                away_cap_secs: u64::MAX,
                away_used_secs: 0,
                want_more: true,
                slices: vec![slice("a", i64::MIN, i64::MAX), slice("b", i64::MAX - 1, i64::MAX), slice("c", 10, 5)],
                added_secs: u64::MAX,
                added_used_secs: 0,
                added: vec![AddedTime {
                    id: "phone/pin/1".into(),
                    child_id: "asha".into(),
                    device_id: "phone".into(),
                    day: today,
                    secs: u64::MAX,
                    by: AddedBy::Pin,
                    at: i64::MAX,
                }],
                requests: vec![],
                wrong_guesses: u32::MAX,
                pin_version: u32::MAX,
                news_seen: u64::MAX,
                pin_unlock: Some(PinUnlock { until: i64::MAX, set_at: i64::MIN }),
            };
            b.handle_report(&report, now).unwrap();
            assert!(b.pool(today).free_secs.unwrap() <= free_before);
            assert!(b.time_left_for("pc", today).unwrap().unwrap() <= free_before);
        }
        b.add_extra_secs(today, u64::MAX);
        b.add_extra_secs(today, u64::MAX);
        let _ = b.pool(today);
        assert!(matches!(b.time_left_for("nobody", today), Err(MasterError::UnknownDevice { .. })));
        assert!(matches!(b.device_view("nobody", today), Err(MasterError::UnknownDevice { .. })));
    }

    #[test]
    fn rules_must_be_valid_and_newer() {
        let mut b = book();
        let mut same = b.rules();
        assert!(matches!(b.set_rules(same.clone()), Err(MasterError::RulesNotNewer { .. })));
        same.version = 2;
        same.lease_minutes = 0;
        assert!(matches!(b.set_rules(same.clone()), Err(MasterError::InvalidRules { .. })));
        same.lease_minutes = 5;
        assert!(b.set_rules(same).is_ok());
        assert_eq!(b.rules().version, 2);
        let err = b.handle_report(
            &Report {
                child_id: "someone else".into(),
                device_id: "x".into(),
                message_number: 1,
                sent_at: 0,
                rules_version: 1,
                grant_number: 0,
                away_percent: 0,
                day: 0,
                used_secs: 0,
                lease_total_secs: 0,
                away_cap_secs: 0,
                away_used_secs: 0,
                want_more: true,
                slices: vec![],
                added_secs: 0,
                added_used_secs: 0,
                added: vec![],
                requests: vec![],
                wrong_guesses: 0,
                pin_version: 0,
                news_seen: 0,
                pin_unlock: None,
            },
            0,
        );
        assert!(matches!(err, Err(MasterError::WrongChild { .. })));
        assert!(!err.unwrap_err().to_string().contains(" - "));
    }
}
