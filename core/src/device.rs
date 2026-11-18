//! The device side: everything one device keeps for one child.
//!
//! A [`DeviceState`] is created from the grant the master makes at pairing
//! and then kept, and saved after every change, by the Windows agent or the
//! Android service. It records use, decides, writes reports and takes
//! grants. The PC that runs the master keeps one too, for each child that
//! uses it, and talks to the master the same way, only without a network.
//!
//! It also holds the ways out on the device: the code secret and the PIN
//! record, the guardian's lock or unlock, the wrong guesses, the time added
//! on the device, and the child's requests for more time.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::codes::{CODE_SECRET_BYTES, CodeCheck, Secret, check_code, check_secret, code_digits};
use crate::decide::{Decision, Holding, MasterWord, decide};
use crate::extra::{
    AddedBy, AddedTime, MAX_ASK_MINUTES, MAX_EXTRA_MINUTES, MAX_REASON_CHARS, MAX_REQUESTS_PER_DAY,
    RequestAnswer, RequestView, TimeRequest,
};
use crate::guardian::{GuardianSetting, MAX_UNLOCK_SECS, PinUnlock, Standing, TryCounter};
use crate::messages::{Grant, Report, Slice};
use crate::pin::{PinAction, PinRecord, pin_is_well_formed};
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

/// What became of a code or a PIN typed on the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TryOutcome {
    /// Right: time was added on the device, for today. It can be spent at
    /// once.
    Added {
        /// The time added, as it will be reported.
        added: AddedTime,
    },
    /// Right: the device is unlocked until this moment.
    Unlocked {
        /// Unix seconds.
        until: i64,
    },
    /// Right: "release this device" is confirmed. The service releases it.
    ReleaseConfirmed,
    /// Wrong. Counted as a wrong guess.
    Wrong {
        /// Wrong guesses on this device since the last right PIN.
        wrong_guesses: u32,
        /// When the next try may be made, if a wait is now in force.
        wait_until: Option<i64>,
    },
    /// Not tried: wrong guesses have set a wait. Not counted.
    MustWait {
        /// When the next try may be made.
        until: i64,
    },
    /// Not the right shape (8 digits for a code, 4 to 8 for a PIN). Not
    /// counted.
    Malformed,
    /// A right code that was already used on this device today. Not
    /// counted.
    AlreadyUsed,
    /// This device has no code secret, or no PIN record. Not counted.
    NotSet,
}

/// What a device refuses to do, in plain words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceError {
    /// A code secret must be exactly 32 bytes.
    SecretWrongLength {
        /// The length given, in bytes.
        given: u32,
    },
    /// The minutes are outside what is allowed.
    MinutesOutOfRange {
        /// The minutes given.
        minutes: u32,
        /// The most allowed.
        most: u32,
    },
    /// The reason given with a request is too long.
    ReasonTooLong {
        /// Its length, in characters.
        chars: u32,
    },
    /// Today has no limit, so there is no time to ask for.
    NoLimitToday,
    /// A request from this device is still waiting for an answer.
    RequestWaiting,
    /// This device has made as many requests today as it may.
    TooManyRequests,
    /// An unlock must end after now and at most a day ahead.
    UnlockOutOfRange,
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::SecretWrongLength { given } => write!(
                f,
                "A code secret must be {CODE_SECRET_BYTES} bytes long; this one is {given} bytes."
            ),
            DeviceError::MinutesOutOfRange { minutes, most } => {
                write!(f, "{minutes} minutes is not allowed here: it must be from 1 to {most}.")
            }
            DeviceError::ReasonTooLong { chars } => write!(
                f,
                "The reason is {chars} characters long; please keep it to {MAX_REASON_CHARS}."
            ),
            DeviceError::NoLimitToday => write!(f, "Today has no time limit, so there is no need to ask."),
            DeviceError::RequestWaiting => {
                write!(f, "A request from this device is still waiting for an answer.")
            }
            DeviceError::TooManyRequests => write!(
                f,
                "This device has asked {MAX_REQUESTS_PER_DAY} times today, which is as many as it may."
            ),
            DeviceError::UnlockOutOfRange => {
                write!(f, "An unlock must end later than now and at most a day ahead.")
            }
        }
    }
}

impl std::error::Error for DeviceError {}

/// A request made on this device, and whether the master has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OwnRequest {
    request: TimeRequest,
    confirmed: bool,
}

/// Everything one device keeps for one child. Its `Debug` hides the code
/// secret and the PIN record.
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
    /// This device's code secret, or empty.
    #[serde(default)]
    code_secret: Secret,
    /// The PIN record, if the master set one.
    #[serde(default)]
    pin: Option<PinRecord>,
    /// The version of the PIN record held, 0 for none yet.
    #[serde(default)]
    pin_version: u32,
    /// The newest lock or unlock from the master.
    #[serde(default)]
    guardian: GuardianSetting,
    /// An unlock set with the PIN on this device.
    #[serde(default)]
    pin_unlock: Option<PinUnlock>,
    /// Wrong guesses of codes and PIN, and the wait they set.
    #[serde(default)]
    tries: TryCounter,
    /// The number of the last time added with the PIN.
    #[serde(default)]
    last_pin_add: u64,
    /// Time added on the device that the master has not confirmed.
    #[serde(default)]
    unsent_added: Vec<AddedTime>,
    /// The number of the last request made.
    #[serde(default)]
    last_request: u64,
    /// Requests made on this device, until the master has them and their day
    /// is over.
    #[serde(default)]
    own_requests: Vec<OwnRequest>,
    /// The requests of the newest grant's day, with their answers.
    #[serde(default)]
    requests_seen: Vec<RequestView>,
    /// The news number of the newest grant taken.
    #[serde(default)]
    news_taken: u64,
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
            code_secret: Secret::default(),
            pin: None,
            pin_version: 0,
            guardian: GuardianSetting::default(),
            pin_unlock: None,
            tries: TryCounter::default(),
            last_pin_add: 0,
            unsent_added: Vec::new(),
            last_request: 0,
            own_requests: Vec::new(),
            requests_seen: Vec::new(),
            news_taken: 0,
        };
        state.take_grant(welcome, now);
        state
    }

    /// Takes this device's code secret: the 32 bytes made by the caller at
    /// pairing and also given to the master
    /// ([`crate::master::MasterBook::set_code_secret`]). Without it the
    /// device refuses codes.
    pub fn set_code_secret(&mut self, secret: Vec<u8>) -> Result<(), DeviceError> {
        check_secret(&secret).map_err(|_| DeviceError::SecretWrongLength {
            given: u32::try_from(secret.len()).unwrap_or(u32::MAX),
        })?;
        self.code_secret = Secret::new(secret);
        Ok(())
    }

    /// True when the device holds a code secret.
    pub fn has_code_secret(&self) -> bool {
        self.code_secret.is_set()
    }

    /// True when the device holds a PIN record.
    pub fn has_pin(&self) -> bool {
        self.pin.is_some()
    }

    /// The version of the PIN record the device holds, 0 for none yet.
    pub fn pin_version(&self) -> u32 {
        self.pin_version
    }

    /// Wrong guesses of codes and PIN on this device since the last right
    /// PIN.
    pub fn wrong_guesses(&self) -> u32 {
        self.tries.wrong
    }

    /// The newest lock or unlock from the master this device has taken.
    pub fn guardian_setting(&self) -> GuardianSetting {
        self.guardian
    }

    /// Time added on the device that the master has not yet confirmed.
    pub fn unsent_added(&self) -> Vec<AddedTime> {
        self.unsent_added.clone()
    }

    /// How the guardian's actions and the ways out stand on this device at
    /// `now`, as [`decide`] takes it.
    pub fn standing_at(&self, now: i64) -> Standing {
        let today = self.holding_at(now).day;
        Standing {
            from_master: self.guardian,
            pin_unlock: self.pin_unlock,
            wrong_guesses: self.tries.wrong,
            try_wait_until: self.tries.wait_until,
            may_ask: self.ask_refusal(today).is_none(),
            has_pin: self.pin.is_some(),
            has_code_secret: self.code_secret.is_set(),
        }
    }

    /// The requests for more time made for today on any of the child's
    /// devices, as far as this device knows, with their answers, oldest
    /// first. A request made here that has not reached the master yet is
    /// shown as waiting.
    pub fn requests_today(&self, now: i64) -> Vec<RequestView> {
        let today = self.holding_at(now).day;
        let mut views: Vec<RequestView> =
            self.requests_seen.iter().filter(|v| v.request.day == today).cloned().collect();
        for own in self.own_requests.iter().filter(|o| o.request.day == today) {
            if !views.iter().any(|v| v.request.id == own.request.id) {
                views.push(RequestView {
                    request: own.request.clone(),
                    answer: RequestAnswer::Waiting,
                    answered_at: None,
                    reached_master: own.confirmed,
                });
            }
        }
        views.sort_by(|a, b| (a.request.asked_at, &a.request.id).cmp(&(b.request.asked_at, &b.request.id)));
        views
    }

    /// Why this device may not ask for more time on `day` now, if it may
    /// not.
    fn ask_refusal(&self, day: i64) -> Option<DeviceError> {
        let mine: Vec<&OwnRequest> = self.own_requests.iter().filter(|o| o.request.day == day).collect();
        let waiting = mine.iter().any(|o| {
            !self
                .requests_seen
                .iter()
                .any(|v| v.request.id == o.request.id && v.answer != RequestAnswer::Waiting)
        });
        if waiting {
            Some(DeviceError::RequestWaiting)
        } else if mine.len() as u64 >= MAX_REQUESTS_PER_DAY as u64 {
            Some(DeviceError::TooManyRequests)
        } else {
            None
        }
    }

    /// The child asks for more time today: `minutes` (1 to 240) and a short
    /// reason, possibly empty. The request goes to the master with the next
    /// report, and with every report after until the master confirms it, so
    /// it survives a device out of reach and lost messages. One request at a
    /// time may wait for an answer, and a device may ask five times a day.
    pub fn ask_for_time(&mut self, minutes: u32, reason: &str, now: i64) -> Result<TimeRequest, DeviceError> {
        self.move_to(now);
        if minutes == 0 || minutes > MAX_ASK_MINUTES {
            return Err(DeviceError::MinutesOutOfRange { minutes, most: MAX_ASK_MINUTES });
        }
        let reason = reason.trim();
        let chars = reason.chars().count() as u64;
        if chars > MAX_REASON_CHARS as u64 {
            return Err(DeviceError::ReasonTooLong { chars: u32::try_from(chars).unwrap_or(u32::MAX) });
        }
        let day = self.holding.day;
        if self.rules.budget_secs_on(day).is_none() {
            return Err(DeviceError::NoLimitToday);
        }
        if let Some(refusal) = self.ask_refusal(day) {
            return Err(refusal);
        }
        self.last_request += 1;
        let request = TimeRequest {
            id: format!("{}/ask/{}", self.device_id, self.last_request),
            child_id: self.child_id.clone(),
            device_id: self.device_id.clone(),
            day,
            minutes,
            reason: reason.to_string(),
            asked_at: now,
        };
        self.own_requests.push(OwnRequest { request: request.clone(), confirmed: false });
        Ok(request)
    }

    /// Adds `secs` on the device for its day, and records it for the master.
    fn add_on_device(&mut self, id: String, secs: u64, by: AddedBy, now: i64) -> AddedTime {
        self.holding.added_secs = self.holding.added_secs.saturating_add(secs);
        let added = AddedTime {
            id,
            child_id: self.child_id.clone(),
            device_id: self.device_id.clone(),
            day: self.holding.day,
            secs,
            by,
            at: now,
        };
        self.unsent_added.push(added.clone());
        added
    }

    /// The child types a one time code. A right code adds its minutes on the
    /// device for today, once; a code for another device or another day is
    /// simply wrong. A well formed wrong code counts as a wrong guess; one
    /// that is not 8 digits does not. No try is made while a wait is in
    /// force.
    pub fn try_code(&mut self, typed: &str, now: i64) -> TryOutcome {
        self.move_to(now);
        if !self.code_secret.is_set() {
            return TryOutcome::NotSet;
        }
        if code_digits(typed).is_none() {
            return TryOutcome::Malformed;
        }
        if let Some(until) = self.tries.wait_at(now) {
            return TryOutcome::MustWait { until };
        }
        let day = self.holding.day;
        match check_code(self.code_secret.bytes(), &self.device_id, day, typed) {
            CodeCheck::Malformed => TryOutcome::Malformed,
            CodeCheck::Wrong => {
                self.tries.wrong_guess(now);
                TryOutcome::Wrong { wrong_guesses: self.tries.wrong, wait_until: self.tries.wait_at(now) }
            }
            CodeCheck::Right { selector, minutes } => {
                if self.holding.codes_used.contains(&selector) {
                    return TryOutcome::AlreadyUsed;
                }
                self.holding.codes_used.push(selector);
                let id = format!("{}/code/{day}/{selector:02}", self.device_id);
                let added = self.add_on_device(id, minutes as u64 * 60, AddedBy::Code { selector }, now);
                TryOutcome::Added { added }
            }
        }
    }

    /// A guardian types the PIN on the device to do `action`. A right PIN
    /// clears the wrong guesses. A well formed wrong PIN counts as a wrong
    /// guess; one that is not 4 to 8 digits does not. No try is made while a
    /// wait is in force, and none on a device without a PIN record. An action
    /// that could not be done (minutes out of range, an unlock in the past or
    /// more than a day ahead) is refused before the PIN is looked at.
    pub fn try_pin(&mut self, pin: &str, action: PinAction, now: i64) -> Result<TryOutcome, DeviceError> {
        self.move_to(now);
        match action {
            PinAction::AddMinutes { minutes } if minutes == 0 || minutes > MAX_EXTRA_MINUTES => {
                return Err(DeviceError::MinutesOutOfRange { minutes, most: MAX_EXTRA_MINUTES });
            }
            PinAction::UnlockUntil { until } if until <= now || until > now.saturating_add(MAX_UNLOCK_SECS) => {
                return Err(DeviceError::UnlockOutOfRange);
            }
            _ => {}
        }
        let Some(record) = &self.pin else {
            return Ok(TryOutcome::NotSet);
        };
        if !pin_is_well_formed(pin) {
            return Ok(TryOutcome::Malformed);
        }
        if let Some(until) = self.tries.wait_at(now) {
            return Ok(TryOutcome::MustWait { until });
        }
        if !record.matches(pin) {
            self.tries.wrong_guess(now);
            return Ok(TryOutcome::Wrong { wrong_guesses: self.tries.wrong, wait_until: self.tries.wait_at(now) });
        }
        self.tries.clear();
        Ok(match action {
            PinAction::AddMinutes { minutes } => {
                self.last_pin_add += 1;
                let id = format!("{}/pin/{}", self.device_id, self.last_pin_add);
                TryOutcome::Added { added: self.add_on_device(id, minutes as u64 * 60, AddedBy::Pin, now) }
            }
            PinAction::UnlockUntil { until } => {
                self.pin_unlock = Some(PinUnlock { until, set_at: now });
                TryOutcome::Unlocked { until }
            }
            PinAction::ConfirmRelease => TryOutcome::ReleaseConfirmed,
        })
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
            // Requests of earlier days that the master has are done with.
            let today = self.holding.day;
            self.own_requests.retain(|o| o.request.day >= today || !o.confirmed);
        }
    }

    /// Decides for the app in front (see [`decide`]) and keeps the warnings
    /// given, so each warning point comes once.
    pub fn decide(&mut self, now: i64, app: Option<&str>) -> Decision {
        self.move_to(now);
        let standing = self.standing_at(now);
        let decision = decide(&self.rules, &self.holding, &standing, now, app);
        if decision.day == self.holding.day {
            self.holding.warned = decision.warned.clone();
        }
        decision
    }

    /// Records use of `app_id` from `start` to `end`, cut at every day
    /// boundary, and returns the slices made. Counted time is spent from the
    /// lease first, then from the time added on the device, then from the
    /// away allowance. Use while a guardian's unlock keeps the device open
    /// counts the same way, and past all of it, if need be.
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
    /// every slice, piece of time added on the device and request not yet
    /// confirmed, the wrong guesses, and, with `want_more`, a request to top
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
            added_secs: self.holding.added_secs,
            added_used_secs: self.holding.added_used_secs,
            added: self.unsent_added.clone(),
            requests: self.own_requests.iter().filter(|o| !o.confirmed).map(|o| o.request.clone()).collect(),
            wrong_guesses: self.tries.wrong,
            pin_version: self.pin_version,
            news_seen: self.news_taken,
            pin_unlock: self.pin_unlock.filter(|p| p.until > now),
        }
    }

    /// Takes a grant from the master. Confirmations, and a PIN record or a
    /// guardian's setting newer than the one held, are used from any grant.
    /// The rest only from a grant newer than any taken before; a grant for an
    /// earlier day changes only the rules and the away share. When a taken
    /// grant says `acknowledge_now`, send a report at once.
    pub fn take_grant(&mut self, grant: &Grant, now: i64) -> GrantOutcome {
        if grant.child_id != self.child_id || grant.device_id != self.device_id {
            return GrantOutcome::NotForThisDevice;
        }
        if !grant.confirmed_slices.is_empty() {
            let confirmed: BTreeSet<&str> = grant.confirmed_slices.iter().map(String::as_str).collect();
            self.unconfirmed.retain(|s| !confirmed.contains(s.id.as_str()));
        }
        if !grant.confirmed_added.is_empty() {
            let confirmed: BTreeSet<&str> = grant.confirmed_added.iter().map(String::as_str).collect();
            self.unsent_added.retain(|a| !confirmed.contains(a.id.as_str()));
        }
        for own in &mut self.own_requests {
            if grant.confirmed_requests.contains(&own.request.id) {
                own.confirmed = true;
            }
        }
        if let Some(update) = &grant.pin
            && update.version > self.pin_version
        {
            self.pin_version = update.version;
            // A record this library would not make is not trusted: the device
            // then refuses PIN actions until a sound one comes.
            self.pin = update.record.clone().filter(PinRecord::is_sound);
        }
        if grant.guardian.version > self.guardian.version {
            self.guardian = grant.guardian;
        }
        if grant.number <= self.grant_taken {
            return GrantOutcome::Stale;
        }
        self.news_taken = self.news_taken.max(grant.news);
        self.requests_seen = grant.requests.clone();
        // Start any new day with the terms held so far, then take the new away
        // share and rules together. A day the new rules start is started with
        // both, never with the old share and the new rules: the master reserves
        // for the pairs it sent, and only those.
        self.move_to(now);
        self.grant_taken = grant.number;
        self.holding.away_percent = grant.away_percent;
        if grant.rules.version > self.rules.version {
            self.rules = grant.rules.clone();
        }
        self.move_to(now);
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
