//! The device side: everything one device keeps for one child.
//!
//! A [`DeviceState`] is created from the grant the master makes at pairing
//! and then kept, and saved after every change, by the Windows agent or the
//! Android service. It records use, decides, writes reports and takes
//! grants. The PC that runs the master keeps one too, for each child that
//! uses it, and talks to the master the same way, only without a network.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::decide::{Decision, Holding, MasterWord, decide};
use crate::messages::{Grant, Report, Slice};
use crate::rules::RuleSet;

/// What became of a grant handed to [`DeviceState::take_grant`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantOutcome {
    /// Taken: the device now holds what it says.
    Taken,
    /// Older than a grant already taken. Only its slice confirmations were
    /// used.
    Stale,
    /// For another child or another device. Ignored.
    NotForThisDevice,
}

/// Everything one device keeps for one child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceState {
    child_id: String,
    device_id: String,
    rules: RuleSet,
    holding: Holding,
    grant_taken: u64,
    last_message: u64,
    last_slice: u64,
    unconfirmed: Vec<Slice>,
}

impl DeviceState {
    /// A device's state, made from the grant the master makes when the device
    /// is paired ([`crate::master::MasterBook::add_device`]).
    pub fn from_pairing(welcome: &Grant, now: i64) -> DeviceState {
        let rules = welcome.rules.clone();
        let day = rules.calendar().day_of(now);
        let holding = Holding::fresh(&rules, day, welcome.away_percent);
        let mut state = DeviceState {
            child_id: welcome.child_id.clone(),
            device_id: welcome.device_id.clone(),
            rules,
            holding,
            grant_taken: 0,
            last_message: 0,
            last_slice: 0,
            unconfirmed: Vec::new(),
        };
        state.take_grant(welcome, now);
        state
    }

    /// The child.
    pub fn child_id(&self) -> String {
        self.child_id.clone()
    }

    /// This device.
    pub fn device_id(&self) -> String {
        self.device_id.clone()
    }

    /// The rules this device holds.
    pub fn rules(&self) -> RuleSet {
        self.rules.clone()
    }

    /// The version of the rules this device holds.
    pub fn rules_version(&self) -> u32 {
        self.rules.version
    }

    /// What this device holds, as it stands at `now`.
    pub fn holding_at(&self, now: i64) -> Holding {
        self.holding.for_day(&self.rules, self.rules.calendar().day_of(now))
    }

    /// The number of the newest grant taken.
    pub fn grant_taken(&self) -> u64 {
        self.grant_taken
    }

    /// Slices not yet confirmed by the master.
    pub fn unconfirmed_slices(&self) -> Vec<Slice> {
        self.unconfirmed.clone()
    }

    /// Moves the day forward to the home day of `now`, if it has changed. The
    /// day never moves back, even if the clock does.
    fn move_to(&mut self, now: i64) {
        let day = self.rules.calendar().day_of(now);
        if day > self.holding.day {
            self.holding = self.holding.for_day(&self.rules, day);
        }
    }

    /// Decides for the app in front (see [`decide`]) and keeps the warnings
    /// given, so each warning point comes once.
    pub fn decide(&mut self, now: i64, app: Option<&str>) -> Decision {
        self.move_to(now);
        let decision = decide(&self.rules, &self.holding, now, app);
        if decision.day == self.holding.day {
            self.holding.warned = decision.warned.clone();
        }
        decision
    }

    /// Records use of `app_id` from `start` to `end`, cut at every day
    /// boundary, and returns the slices made. Counted time is spent from the
    /// lease first, then from the away allowance.
    ///
    /// The day never moves back: use that the calendar puts on a day before
    /// the one this device is in (after a clock was set back, or new rules
    /// moved the day start) counts on the device's day. Record use promptly,
    /// before the day changes, so that it counts on the day it happened.
    pub fn record_use(&mut self, app_id: &str, start: i64, end: i64) -> Vec<Slice> {
        let counts = !self.rules.is_always_allowed(app_id);
        let mut made = Vec::new();
        for span in self.rules.calendar().split(start, end) {
            if span.day > self.holding.day {
                self.holding = self.holding.for_day(&self.rules, span.day);
            }
            let day = self.holding.day;
            if counts {
                self.holding.spend(span.end.saturating_sub(span.start).max(0) as u64);
            }
            self.last_slice += 1;
            let slice = Slice {
                id: format!("{}/{}", self.device_id, self.last_slice),
                child_id: self.child_id.clone(),
                device_id: self.device_id.clone(),
                app_id: app_id.to_string(),
                day,
                start: span.start,
                end: span.end,
                counts,
            };
            self.unconfirmed.push(slice.clone());
            made.push(slice);
        }
        made
    }

    /// Writes the next report: what this device holds and has used today,
    /// every slice not yet confirmed, and, with `want_more`, a request to top
    /// the lease up to one lease length.
    pub fn report(&mut self, now: i64, want_more: bool) -> Report {
        self.move_to(now);
        self.last_message += 1;
        Report {
            child_id: self.child_id.clone(),
            device_id: self.device_id.clone(),
            message_number: self.last_message,
            sent_at: now,
            rules_version: self.rules.version,
            grant_number: self.grant_taken,
            away_percent: self.holding.away_percent,
            day: self.holding.day,
            used_secs: self.holding.used_secs(),
            lease_total_secs: self.holding.lease_total_secs,
            away_cap_secs: self.holding.away_cap_secs,
            away_used_secs: self.holding.away_used_secs,
            want_more,
            slices: self.unconfirmed.clone(),
        }
    }

    /// Takes a grant from the master. Slice confirmations are used from any
    /// grant. The rest only from a grant newer than any taken before; a
    /// grant for an earlier day changes only the rules and the away share.
    /// When a taken grant says `acknowledge_now`, send a report at once.
    pub fn take_grant(&mut self, grant: &Grant, now: i64) -> GrantOutcome {
        if grant.child_id != self.child_id || grant.device_id != self.device_id {
            return GrantOutcome::NotForThisDevice;
        }
        if !grant.confirmed_slices.is_empty() {
            let confirmed: BTreeSet<&str> = grant.confirmed_slices.iter().map(String::as_str).collect();
            self.unconfirmed.retain(|s| !confirmed.contains(s.id.as_str()));
        }
        if grant.number <= self.grant_taken {
            return GrantOutcome::Stale;
        }
        self.move_to(now);
        self.grant_taken = grant.number;
        if grant.rules.version > self.rules.version {
            self.rules = grant.rules.clone();
            self.move_to(now);
        }
        self.holding.away_percent = grant.away_percent;
        if grant.day == self.holding.day {
            self.holding.lease_total_secs = self.holding.lease_total_secs.max(grant.lease_total_secs);
            self.holding.away_cap_secs = grant.away_cap_secs;
            self.holding.master_word = grant.left_for_you_secs.map(|left_secs| MasterWord {
                left_secs,
                used_then_secs: grant.used_secs_reported,
                held_elsewhere_secs: grant.held_elsewhere_secs,
            });
        }
        GrantOutcome::Taken
    }

    /// Call when the master could not be reached. The device then stops
    /// trusting what the master last said about the time left, so that its
    /// warnings follow the time in hand.
    pub fn lost_contact(&mut self) {
        self.holding.master_word = None;
    }
}
