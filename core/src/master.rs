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

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::messages::{Grant, Report, Slice};
use crate::rules::{RuleProblem, RuleSet};

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
    /// time. Normally 0; above 0 after the budget was cut.
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
}

impl LastReport {
    fn from_report(r: &Report) -> LastReport {
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
        }
    }

    fn lease_used_secs(&self) -> u64 {
        self.used_secs.saturating_sub(self.away_used_secs)
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
}

impl DeviceRecord {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DayRecord {
    day: i64,
    extra_secs: u64,
    /// Every slice handed over for this day, by id.
    slices: BTreeMap<String, Slice>,
    /// Counted seconds per device.
    counted: BTreeMap<String, u64>,
}

/// What a device may still spend on one day, worked out by the master.
struct Exposure {
    /// The device may hold rules with no limit that day.
    unbounded: bool,
    lease_total_secs: u64,
    lease_used_secs: u64,
    away_cap_secs: u64,
    away_used_secs: u64,
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
                .saturating_add(self.away_cap_secs.saturating_sub(self.away_used_secs)),
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

    /// Takes new rules. They reach each device with its next grant.
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
        self.forget_unused_rules();
        Ok(())
    }

    /// Pairs a device and returns its first grant, from which the device
    /// makes its state ([`crate::device::DeviceState::from_pairing`]).
    pub fn add_device(&mut self, device_id: &str, kind: DeviceKind, now: i64) -> Result<Grant, MasterError> {
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
        });
        let index = self.devices.len() - 1;
        let day = self.today(now);
        Ok(self.make_grant(index, day, Ask::default(), now, Vec::new()))
    }

    /// The ids of the paired devices.
    pub fn device_ids(&self) -> Vec<String> {
        self.devices.iter().map(|d| d.device_id.clone()).collect()
    }

    /// Handles a report from a device and returns the grant to send back.
    ///
    /// Slices are counted once each, whatever the number of times they come.
    /// The figures of the report are kept if it is the newest from that
    /// device. The grant tops the lease up to one lease length when the
    /// device asked and the pool allows, and carries the device's away
    /// allowance and the current rules. What the pool cannot give is kept as
    /// the device's unmet need, and the away allowances of phones that report
    /// are lowered to meet it (see [`MasterBook::wanted_reports`]).
    ///
    /// Save the book before sending the grant: the grant must be on record
    /// before the device can spend it.
    pub fn handle_report(&mut self, report: &Report, now: i64) -> Result<Grant, MasterError> {
        if report.child_id != self.child_id {
            return Err(MasterError::WrongChild {
                expected: self.child_id.clone(),
                given: report.child_id.clone(),
            });
        }
        let index = self
            .devices
            .iter()
            .position(|d| d.device_id == report.device_id)
            .ok_or_else(|| MasterError::UnknownDevice { device_id: report.device_id.clone() })?;

        self.clock = self.clock.max(now);
        let confirmed = self.take_slices(&report.device_id, &report.slices);

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
            device.last_report = Some(LastReport::from_report(report));
            device.unacknowledged.retain(|g| g.number > report.grant_number);
        }
        let day = device.last_report.as_ref().map_or(report.day, |r| r.day);
        self.forget_unused_rules();

        // Only the newest report, or a repeat of it, may ask for more, and
        // only it says what the device still needs.
        let current = newest || same;
        let want_more = report.want_more && current;
        let ask = Ask { want_more, current, answers_message: report.message_number };
        Ok(self.make_grant(index, day, ask, now, confirmed))
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
    /// - devices that are waiting, once time is free for them.
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
        }
        wanted.into_iter().collect()
    }

    /// Time devices asked for on `day` and could not be given yet (needs
    /// asked for within [`NEED_KEPT_SECS`] of the latest report).
    pub fn unmet_need_secs(&self, day: i64) -> u64 {
        total(self.devices.iter().filter_map(|d| d.need_on(day, self.clock)).map(|n| n.secs))
    }

    /// Adds extra time to a day's pool. The ways to grant it (requests, one
    /// time codes, the guardian PIN) are not part of this library yet.
    pub fn add_extra_secs(&mut self, day: i64, secs: u64) {
        let record = self.day_record_mut(day);
        record.extra_secs = record.extra_secs.saturating_add(secs);
    }

    /// One day's pool as it stands.
    pub fn pool(&self, day: i64) -> PoolView {
        let budget = self.current_rules().budget_secs_on(day);
        let extra = self.extra_secs(day);
        let used = total(self.devices.iter().map(|d| self.counted_secs(d, day)));
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

    /// One device's part of a day as it stands.
    pub fn device_view(&self, device_id: &str, day: i64) -> Option<DeviceView> {
        let device = self.devices.iter().find(|d| d.device_id == device_id)?;
        Some(DeviceView {
            device_id: device.device_id.clone(),
            kind: device.kind,
            used_secs: self.counted_secs(device, day),
            reserved_secs: self.exposure(device, day).remaining(),
            giving_back_secs: self.pending_release(device, day, Some(self.clock)),
            last_seen: device.last_seen,
            waiting_secs: device.need_on(day, self.clock).map_or(0, |n| n.secs),
        })
    }

    /// What the master would say is left for a device that day: what it may
    /// still spend from what it holds, plus what the master could still hand
    /// it (free time that is not waiting for an older need of another
    /// device). None when there is no limit.
    pub fn time_left_for(&self, device_id: &str, day: i64) -> Result<Option<u64>, MasterError> {
        let index = self
            .devices
            .iter()
            .position(|d| d.device_id == device_id)
            .ok_or_else(|| MasterError::UnknownDevice { device_id: device_id.to_string() })?;
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

    /// Every slice handed over for a day, in time order.
    pub fn usage(&self, day: i64) -> Vec<Slice> {
        let mut slices: Vec<Slice> = self
            .days
            .iter()
            .filter(|d| d.day == day)
            .flat_map(|d| d.slices.values().cloned())
            .collect();
        slices.sort_by(|a, b| (a.start, &a.id).cmp(&(b.start, &b.id)));
        slices
    }

    /// Drops the record of every day before `day`. Slices for those days that
    /// arrive later are confirmed but no longer counted.
    pub fn forget_days_before(&mut self, day: i64) {
        self.days.retain(|d| d.day >= day);
        self.forgotten_before = Some(self.forgotten_before.map_or(day, |f| f.max(day)));
    }

    // Inner workings.

    fn day_record_mut(&mut self, day: i64) -> &mut DayRecord {
        let at = match self.days.binary_search_by_key(&day, |d| d.day) {
            Ok(at) => at,
            Err(at) => {
                self.days.insert(
                    at,
                    DayRecord { day, extra_secs: 0, slices: BTreeMap::new(), counted: BTreeMap::new() },
                );
                at
            }
        };
        &mut self.days[at]
    }

    fn day_record(&self, day: i64) -> Option<&DayRecord> {
        self.days.binary_search_by_key(&day, |d| d.day).ok().map(|at| &self.days[at])
    }

    fn extra_secs(&self, day: i64) -> u64 {
        self.day_record(day).map_or(0, |d| d.extra_secs)
    }

    /// Counts new slices and returns the ids of all those handed over, which
    /// the device may then forget.
    fn take_slices(&mut self, device_id: &str, slices: &[Slice]) -> Vec<String> {
        let mut confirmed = Vec::with_capacity(slices.len());
        for slice in slices {
            confirmed.push(slice.id.clone());
            let forgotten = self.forgotten_before.is_some_and(|f| slice.day < f);
            if slice.child_id != self.child_id
                || slice.device_id != device_id
                || slice.end <= slice.start
                || forgotten
            {
                continue;
            }
            let record = self.day_record_mut(slice.day);
            if record.slices.contains_key(&slice.id) {
                continue;
            }
            if slice.counts {
                let counted = record.counted.entry(device_id.to_string()).or_insert(0);
                *counted = counted.saturating_add(slice.secs());
            }
            record.slices.insert(slice.id.clone(), slice.clone());
        }
        confirmed
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
        Exposure {
            unbounded: false,
            lease_total_secs: lease_total,
            lease_used_secs: lease_used,
            away_cap_secs: away_cap,
            away_used_secs: away_used,
        }
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
        let used = total(self.devices.iter().map(|d| self.counted_secs(d, day)));
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
    fn make_grant(&mut self, index: usize, day: i64, ask: Ask, now: i64, confirmed_slices: Vec<String>) -> Grant {
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
            confirmed_slices,
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
        let grant = b.handle_report(&pc.report(now, true), now).unwrap();
        pc.take_grant(&grant, now);
        pc.record_use("game", now, now + 300);
        let report = pc.report(now + 300, false);
        b.handle_report(&report, now + 300).unwrap();
        b.handle_report(&report, now + 301).unwrap();
        let mut again = report.clone();
        again.message_number += 7;
        b.handle_report(&again, now + 302).unwrap();
        assert_eq!(b.pool(b.today(now)).used_secs, 300);
        assert_eq!(b.usage(b.today(now)).len(), 1);
    }

    #[test]
    fn a_lost_grant_is_absorbed_by_the_next_one() {
        let mut b = book();
        let now = start();
        let mut pc = DeviceState::from_pairing(&b.add_device("pc", DeviceKind::OnMaster, now).unwrap(), now);
        let lost = b.handle_report(&pc.report(now, true), now).unwrap();
        assert_eq!(lost.lease_total_secs, 10 * MIN);
        // The master keeps the lost lease reserved...
        assert_eq!(b.pool(b.today(now)).reserved_secs, Some(10 * MIN));
        // ...and the next grant is a level, not a second lease on top.
        let next = b.handle_report(&pc.report(now + 5, true), now + 5).unwrap();
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
            };
            b.handle_report(&report, now).unwrap();
            assert!(b.pool(today).free_secs.unwrap() <= free_before);
            assert!(b.time_left_for("pc", today).unwrap().unwrap() <= free_before);
        }
        b.add_extra_secs(today, u64::MAX);
        b.add_extra_secs(today, u64::MAX);
        let _ = b.pool(today);
        assert!(matches!(b.time_left_for("nobody", today), Err(MasterError::UnknownDevice { .. })));
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
            },
            0,
        );
        assert!(matches!(err, Err(MasterError::WrongChild { .. })));
        assert!(!err.unwrap_err().to_string().contains(" - "));
    }
}
