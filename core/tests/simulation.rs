//! A deterministic simulation of one child's days: the master, the PC that
//! runs it, and one or two phones, driven through many generated scenarios.
//!
//! Each scenario comes from a seed and a simple generator of our own, so a
//! failure names its seed and can be run again exactly. A scenario mixes:
//! use on every device, often at the same time; phones going out of reach and
//! coming back without saying so; reports and grants lost on the way;
//! reports sent again and grants arriving late; rule changes (budgets, blocked
//! hours, the away share, the lease length, and in some scenarios the home
//! time zone and the day start); extra time; and day changes.
//!
//! What is checked, at every step:
//!
//! 1. **Never over the limit.** For every day, all the time that counts on
//!    every device together is at most that day's budget plus extra time.
//!    "That day's budget" is the highest budget for the day among the rule
//!    versions any device held, or the master granted with, during the day.
//!    In scenarios whose rules never change that is simply the day's budget:
//!    the slack is zero.
//! 2. **The master never knows less than is true.** For every device and
//!    day, what the master has counted plus what it holds back for that
//!    device covers everything the device really used (beyond time added on
//!    the device that the master has not heard of yet, and use under an
//!    unlock beyond what the device held).
//! 3. **A device in reach is never locked while the master has time for
//!    it.** Right after an exchange that went through both ways, a device
//!    locked for lack of time means the master had nothing left to hand out.
//!    After two such exchanges in a row with the same rules, the master's own
//!    figure of time left for that device is zero as well.
//! 4. **No device holds more than one lease** of unspent lease.
//! 5. **A phone at home gives its away time to a device that waits.** In
//!    most scenarios a hub asks the devices the master wants to hear from to
//!    report, at the end of every step. After such a round in which no
//!    message was lost, a device in reach and in use that is still locked
//!    for lack of time means that no other phone in reach still holds away
//!    time it could give back (unless that phone is waiting itself).
//!
//! 6. **The ways out.** In two scenarios in three a guardian also gives
//!    extra time at the master (some of it twice, by the same id), makes one
//!    time codes that are typed on the devices in and out of reach (some late,
//!    some on the wrong device, some again), adds time and unlocks with the
//!    PIN on the devices, changes the PIN, locks and unlocks from the master,
//!    and answers the children's requests; wrong guesses set waits, and
//!    devices restart from their saved state. Check 1 then reads: the time
//!    that counts on all devices, less what was used beyond what a device
//!    held while an unlock kept it open, is at most the day's highest budget
//!    plus all extra time, given at the master or added on a device, each
//!    piece once. The master never counts more extra time than was given.
//!    Use beyond what a device held happens only under an unlock; a device
//!    under a guardian's lock records no counted use; and the master never
//!    counts less use than a device has handed over.
//!
//! 7. **Devices removed and paired anew.** In some scenarios the guardian
//!    removes a device now and then, after its last report or without one
//!    (a lost phone), and pairs a new one in its place. Check 1 then allows
//!    for the use the removed device had not reported, at most what the
//!    master held back for it: nothing else. The removed device's counted
//!    use stays in the master's days, unchanged; a report from it that turns
//!    up later is refused and changes nothing; its id cannot be paired
//!    again.
//! 8. **What the master took, once.** Every report's outcome is checked
//!    against the simulation's own ledger: each new slice, piece of added
//!    time and request is one a device made, taken once; everything sound in
//!    a delivered report has been taken; and for every device and day, the
//!    master's count is exactly the sum of the slices it returned.
//! 9. **Notes, and the other devices' use.** Devices make notes for the
//!    guardian now and then, carry them in every report until a grant
//!    confirms them, and drop them then. Each new note in an outcome is one
//!    a device made, taken once, and every note a delivered report carried
//!    has been taken by then. Every grant's figure of the other devices' use
//!    is the master's own count for that day of every device but the one it
//!    is for, removed ones among them, and says all of it was on the master
//!    PC exactly when it was.
//!
//! The run fails if the scenarios never take time back from a phone, never
//! reach the moment where check 5 applies, never take one of the ways out,
//! never remove or pair a device, or never return new slices, added time,
//! requests and notes (each path counted separately), so that it cannot pass
//! without testing the path.

use std::collections::{BTreeMap, BTreeSet};

use limo_core::{
    BlockedWindow, DailyBudget, Decision, DeviceKind, DeviceState, GuardedPage, Grant, GrantOutcome, LockReason,
    MAX_NOTES_PER_REPORT, MasterBook, MasterError, OneTimeCode, PhoneNote, PhonePermission, PinAction, PinRecord,
    Report, ReportOutcome, RequestAnswer, RequestReply, RuleSet, SentNote, Slice, TryOutcome, Weekday, next_code,
};

/// A small seeded generator (splitmix64). Enough for picking scenarios.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn range(&mut self, low: i64, high: i64) -> i64 {
        low + self.below((high - low + 1) as u64) as i64
    }
    /// True with this chance in a thousand.
    fn per_mille(&mut self, chance: u64) -> bool {
        self.below(1_000) < chance
    }
    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize]
    }
}

#[derive(Debug, Clone, Copy)]
struct Setup {
    phones: usize,
    /// Chance in a thousand that a phone's report, or the grant back, is lost.
    loss: u64,
    /// Chance in a thousand per step that an old report or grant turns up again.
    repeat: u64,
    /// Chance in a thousand per step that the guardian changes the rules.
    rule_change: u64,
    /// Whether rule changes may move the home time zone or the day start.
    calendar_change: bool,
    /// Chance in a thousand per step that extra time is given.
    extra: u64,
    /// Chance in a thousand per step that a phone's reach changes.
    reach_change: u64,
    days: i64,
    /// Whether a hub asks the devices the master wants to hear from to
    /// report. Without it, time moves only at their own reports.
    hub: bool,
    /// Whether the guardian uses the ways out: extra time with ids, codes,
    /// the PIN, locks and unlocks, and answers to requests.
    ways: bool,
    /// Chance in a thousand per step that a device is removed (and most often
    /// a new one paired in its place).
    removal: u64,
}

#[derive(Debug, Default)]
struct Stats {
    scenarios: u64,
    steps: u64,
    exchanges: u64,
    lost: u64,
    repeats: u64,
    held_up: u64,
    late_grants_taken: u64,
    rule_changes: u64,
    days_checked: u64,
    days_at_the_limit: u64,
    locked_for_time_in_reach: u64,
    strict_checks: u64,
    phone_minutes_away: u64,
    away_use_secs: u64,
    hub_rounds: u64,
    reclaims: u64,
    reclaimed_secs: u64,
    reclaim_checks: u64,
    extra_given: u64,
    extra_repeats: u64,
    codes_made: u64,
    codes_exhausted: u64,
    codes_used_in_reach: u64,
    codes_used_away: u64,
    codes_reused: u64,
    codes_old: u64,
    codes_on_wrong_device: u64,
    random_codes: u64,
    pin_adds_in_reach: u64,
    pin_adds_away: u64,
    pin_unlocks: u64,
    pin_wrong: u64,
    pin_changes: u64,
    waits_hit: u64,
    guardian_locks: u64,
    guardian_unlocks: u64,
    lifts: u64,
    requests_made: u64,
    requests_granted: u64,
    requests_refused: u64,
    over_secs_under_unlock: u64,
    lock_checks: u64,
    restarts: u64,
    count_checks: u64,
    removals_after_report: u64,
    removals_with_unreported_use: u64,
    unreported_secs_lost: u64,
    pairings: u64,
    removed_reports_refused: u64,
    removed_count_checks: u64,
    outcome_slices: u64,
    outcome_added: u64,
    outcome_requests: u64,
    ledger_checks: u64,
    notes_made: u64,
    outcome_notes: u64,
    others_checks: u64,
    others_on_the_pc: u64,
}

struct Device {
    id: String,
    state: DeviceState,
    portable: bool,
    /// Removed from the master: it is gone, and never acts again.
    removed: bool,
    in_reach: bool,
    in_use: bool,
    app: &'static str,
    /// Counted seconds really used, by the device's own home day.
    used: BTreeMap<i64, u64>,
    /// Exchanges in a row that went through both ways with the same rules.
    clean: u32,
    /// The day and away allowance of the last grant taken in such an exchange.
    last_cap: Option<(i64, u64)>,
    /// True when the last two such grants gave the same away allowance.
    same_cap: bool,
    /// Notes for the guardian not yet confirmed, oldest first, kept as the
    /// phone's engine keeps them: saved at once, so a restart keeps them.
    notes: Vec<SentNote>,
    /// The number of the last note made.
    last_note: u64,
}

impl Device {
    fn new(id: String, state: DeviceState, portable: bool) -> Device {
        Device {
            id,
            state,
            portable,
            removed: false,
            in_reach: true,
            in_use: false,
            app: "game",
            used: BTreeMap::new(),
            clean: 0,
            last_cap: None,
            same_cap: false,
            notes: Vec::new(),
            last_note: 0,
        }
    }

    fn count_clean(&mut self, day: i64, away_cap: u64, rules_changed: bool) {
        self.clean = if rules_changed { 0 } else { self.clean + 1 };
        self.same_cap = self.last_cap == Some((day, away_cap));
        self.last_cap = Some((day, away_cap));
    }
}

enum Exchange {
    Lost,
    Through { grant_day: i64, away_cap: u64, rules_changed: bool, acknowledge_now: bool },
}

struct World {
    rng: Rng,
    seed: u64,
    setup: Setup,
    master: MasterBook,
    devices: Vec<Device>,
    /// Every rule version ever made, by version.
    history: BTreeMap<u32, RuleSet>,
    /// For each day, the rule versions that some device held during it, or
    /// that the master granted with for it.
    in_play: BTreeMap<i64, BTreeSet<u32>>,
    old_reports: Vec<Report>,
    old_grants: Vec<(usize, Grant)>,
    /// Messages held up on the way, to arrive later than messages sent after.
    late_reports: Vec<Report>,
    late_grants: Vec<(usize, Grant)>,
    stats: Stats,
    trace: bool,
    /// Each device's code secret.
    secrets: Vec<Vec<u8>>,
    /// The counters of the codes made, by device and day.
    counters: BTreeMap<(usize, i64), Vec<u8>>,
    /// Codes read out to a device, and when the child types them.
    inbox: Vec<(usize, OneTimeCode, i64)>,
    /// Codes that worked, to be typed again now and then.
    used_codes: Vec<(usize, String)>,
    /// The guardian's PIN as last set at the master.
    pin: String,
    /// Extra time given at the master, by day: by `give_extra` once per id,
    /// by `add_extra_secs`, and by granted requests.
    extra_given: BTreeMap<i64, u64>,
    /// Every piece of time added on a device, by id: its day and seconds.
    added: BTreeMap<String, (i64, u64)>,
    /// Counted use beyond what a device held, by device and day.
    over: BTreeMap<(usize, i64), u64>,
    /// Ids given with `give_extra`, to give again.
    given_ids: Vec<String>,
    /// The generator for removing and pairing devices.
    removal_rng: Rng,
    /// Every slice a device made, by id.
    made: BTreeMap<String, Slice>,
    /// The ids of the slices, pieces of added time and requests the master
    /// returned as new, each once.
    taken_slices: BTreeSet<String>,
    taken_added: BTreeSet<String>,
    taken_requests: BTreeSet<String>,
    /// Counted seconds of the slices the master returned, by device and day.
    ledger: BTreeMap<(String, i64), u64>,
    /// Use a removed device had not reported, within what the master held
    /// back for it, by day: the most that removing it can let a day pass
    /// its limit by.
    lost: BTreeMap<i64, u64>,
    /// Devices paired so far, to make new ids.
    paired: u64,
    /// The generator for notes, of its own so that notes change none of the
    /// scenario's other draws.
    note_rng: Rng,
    /// Every note a device made, by id, and the ids the master returned as
    /// new, each once.
    noted: BTreeMap<String, SentNote>,
    taken_notes: BTreeSet<String>,
    /// Notes a device dropped unsent, holding as many as it may keep.
    dropped_notes: BTreeSet<String>,
}

const APPS: [&str; 6] = ["game", "video", "game", "chat", "clock", "casino"];
const OFFSETS: [i32; 7] = [19_800, -18_000, 0, 50_400, -43_200, 20_700, 3_600];

fn random_budget(rng: &mut Rng) -> DailyBudget {
    match rng.below(10) {
        0 => DailyBudget::NoLimit,
        1 => DailyBudget::Minutes(0),
        _ => DailyBudget::Minutes(rng.pick(&[15, 30, 45, 60, 90, 120, 180, 240])),
    }
}

fn random_window(rng: &mut Rng) -> BlockedWindow {
    let mut days: Vec<Weekday> = Weekday::ALL.into_iter().filter(|_| rng.below(2) == 0).collect();
    if days.is_empty() {
        days.push(rng.pick(&Weekday::ALL));
    }
    let start = rng.range(0, 95) as u16 * 15;
    let length = rng.range(1, 40) as u16 * 15;
    let end = (start + length) % 1_440;
    BlockedWindow { days, start_minute: start, end_minute: if end == 0 { 1_440 } else { end } }
}

fn random_rules(rng: &mut Rng, version: u32, offset: i32, day_start: u16) -> RuleSet {
    let mut rules = RuleSet::new(version, offset);
    rules.day_start_minute = day_start;
    for day in Weekday::ALL {
        rules.budgets.set(day, random_budget(rng));
    }
    rules.blocked_hours = (0..rng.below(3)).map(|_| random_window(rng)).collect();
    rules.always_allowed_apps = vec!["clock".into()];
    rules.blocked_apps = vec!["casino".into()];
    rules.away_share_percent = rng.pick(&[0, 25, 50, 50, 50, 75, 100]);
    rules.lease_minutes = rng.pick(&[1, 5, 10, 10, 10, 15, 30]);
    rules
}

impl World {
    fn new(seed: u64) -> World {
        let mut rng = Rng(seed);
        let setup = Setup {
            phones: rng.pick(&[1, 1, 2]),
            loss: rng.pick(&[0, 50, 200, 500]),
            repeat: rng.pick(&[0, 20, 100]),
            rule_change: rng.pick(&[0, 0, 3, 10]),
            calendar_change: rng.below(4) == 0,
            extra: rng.pick(&[0, 2]),
            reach_change: rng.pick(&[5, 20, 60]),
            days: rng.range(2, 3),
            hub: rng.below(4) != 0,
            ways: rng.below(3) != 0,
            removal: 0,
        };
        // Removals draw on a generator of their own, so that they do not
        // change the rest of a scenario's draws until the first removal.
        let mut removal_rng = Rng(seed ^ 0x000D_E71C_E50F_F5E7);
        let setup = Setup { removal: removal_rng.pick(&[0, 0, 1, 3]), ..setup };
        let offset = rng.pick(&OFFSETS);
        let day_start = rng.pick(&[0, 0, 240, 1_380]);
        let rules = random_rules(&mut rng, 1, offset, day_start);
        assert!(rules.is_valid(), "seed {seed}: {:?}", rules.validate());
        let mut master = MasterBook::new("child", rules.clone()).unwrap();
        // Somewhere between 2001 and 2033, at a random moment of the day.
        let start = rng.range(1_000_000_000, 2_000_000_000);
        let mut devices = Vec::new();
        for i in 0..=setup.phones {
            let (id, kind) =
                if i == 0 { ("pc".to_string(), DeviceKind::OnMaster) } else { (format!("phone{i}"), DeviceKind::Portable) };
            let welcome = master.add_device(&id, kind, start).unwrap();
            devices.push(Device::new(id, DeviceState::from_pairing(&welcome, start), kind == DeviceKind::Portable));
        }
        let mut history = BTreeMap::new();
        history.insert(1, rules);
        let mut secrets = Vec::new();
        let mut pin = String::new();
        if setup.ways {
            for device in &mut devices {
                let secret: Vec<u8> = (0..32).map(|_| rng.next() as u8).collect();
                master.set_code_secret(&device.id, secret.clone()).unwrap();
                device.state.set_code_secret(secret.clone()).unwrap();
                secrets.push(secret);
            }
            pin = random_pin(&mut rng);
            master.set_pin_record(quick_pin(&pin)).unwrap();
        }
        let mut world = World {
            rng,
            seed,
            setup,
            master,
            devices,
            history,
            in_play: BTreeMap::new(),
            old_reports: Vec::new(),
            old_grants: Vec::new(),
            late_reports: Vec::new(),
            late_grants: Vec::new(),
            stats: Stats::default(),
            trace: std::env::var("LIMO_SIM_TRACE").is_ok(),
            secrets,
            counters: BTreeMap::new(),
            inbox: Vec::new(),
            used_codes: Vec::new(),
            pin,
            extra_given: BTreeMap::new(),
            added: BTreeMap::new(),
            over: BTreeMap::new(),
            given_ids: Vec::new(),
            removal_rng,
            made: BTreeMap::new(),
            taken_slices: BTreeSet::new(),
            taken_added: BTreeSet::new(),
            taken_requests: BTreeSet::new(),
            ledger: BTreeMap::new(),
            lost: BTreeMap::new(),
            paired: 0,
            note_rng: Rng(seed ^ 0x0000_7E5E_DA7E_0F0F),
            noted: BTreeMap::new(),
            taken_notes: BTreeSet::new(),
            dropped_notes: BTreeSet::new(),
        };
        let day = world.master.today(start);
        world.in_play.entry(day).or_default().insert(1);
        world.run(start);
        world
    }

    fn log(&self, text: impl FnOnce() -> String) {
        if self.trace {
            println!("{}", text());
        }
    }

    fn note_held(&mut self, i: usize, now: i64) {
        let device = &self.devices[i];
        let day = device.state.holding_at(now).day;
        self.in_play.entry(day).or_default().insert(device.state.rules_version());
    }

    /// A report and its grant, then, while a taken grant asks to be
    /// acknowledged at once, another report and grant. Losses as set up.
    fn exchange(&mut self, i: usize, now: i64, want_more: bool) -> Exchange {
        let mut result = self.exchange_once(i, now, want_more);
        let mut changed = false;
        for _ in 0..3 {
            match result {
                Exchange::Through { acknowledge_now: true, rules_changed, .. } => {
                    changed |= rules_changed;
                    result = self.exchange_once(i, now, want_more);
                }
                _ => break,
            }
        }
        if let Exchange::Through { rules_changed, .. } = &mut result {
            *rules_changed |= changed;
        }
        result
    }

    fn exchange_once(&mut self, i: usize, now: i64, want_more: bool) -> Exchange {
        self.stats.exchanges += 1;
        self.note_held(i, now);
        let mut report = self.devices[i].state.report(now, want_more);
        report.notes = self.devices[i].notes.clone();
        self.log(|| format!("{now} {} report #{} day {} used {} lease {} cap {} away_used {} grant {} rules {} want {} today {} slices {:?}",
            report.device_id, report.message_number, report.day, report.used_secs, report.lease_total_secs,
            report.away_cap_secs, report.away_used_secs, report.grant_number, report.rules_version,
            report.want_more, self.master.today(now),
            report.slices.iter().map(|s| (s.day, s.secs())).collect::<Vec<_>>()));
        let lossy = self.devices[i].portable;
        if self.old_reports.len() < 64 {
            self.old_reports.push(report.clone());
        } else {
            let at = self.rng.below(64) as usize;
            self.old_reports[at] = report.clone();
        }
        if lossy && self.rng.per_mille(self.setup.loss) {
            self.stats.lost += 1;
            if self.rng.below(2) == 0 {
                self.stats.held_up += 1;
                self.late_reports.push(report);
            }
            self.devices[i].state.lost_contact();
            self.devices[i].clean = 0;
            return Exchange::Lost;
        }
        let waiting_before = self.master.unmet_need_secs(report.day);
        let outcome = self.master.handle_report(&report, now).unwrap();
        self.check_outcome(&report, &outcome);
        let grant = outcome.grant;
        self.log(|| format!("{now} grant #{} for {} day {} lease {} cap {} percent {} rules {} left {:?} ack {} waiting {}",
            grant.number, grant.device_id, grant.day, grant.lease_total_secs, grant.away_cap_secs,
            grant.away_percent, grant.rules.version, grant.left_for_you_secs, grant.acknowledge_now, waiting_before));
        self.in_play.entry(grant.day).or_default().insert(grant.rules.version);
        // Time taken back from a phone for a device that waits.
        if grant.acknowledge_now && waiting_before > 0 && grant.rules.version == report.rules_version {
            self.stats.reclaims += 1;
            self.stats.reclaimed_secs += report.away_cap_secs.saturating_sub(grant.away_cap_secs);
        }
        if self.old_grants.len() < 64 {
            self.old_grants.push((i, grant.clone()));
        } else {
            let at = self.rng.below(64) as usize;
            self.old_grants[at] = (i, grant.clone());
        }
        if lossy && self.rng.per_mille(self.setup.loss) {
            self.stats.lost += 1;
            if self.rng.below(2) == 0 {
                self.stats.held_up += 1;
                self.late_grants.push((i, grant));
            }
            self.devices[i].state.lost_contact();
            self.devices[i].clean = 0;
            return Exchange::Lost;
        }
        let before = self.devices[i].state.rules_version();
        let outcome = self.take(i, &grant, now);
        assert_eq!(outcome, GrantOutcome::Taken, "seed {}: a fresh grant must be taken", self.seed);
        self.note_held(i, now);
        let rules_changed = before != grant.rules.version;
        self.check_one_lease(i, now);
        Exchange::Through {
            grant_day: grant.day,
            away_cap: grant.away_cap_secs,
            rules_changed,
            acknowledge_now: grant.acknowledge_now,
        }
    }

    /// Check 8: what the master took from a report it handled, against the
    /// simulation's ledger. Each new slice, piece of added time and request
    /// is one a device made, taken once; and everything sound that the
    /// report carried has been taken by now, in this report or before.
    fn check_outcome(&mut self, report: &Report, outcome: &ReportOutcome) {
        let seed = self.seed;
        for slice in &outcome.new_slices {
            assert_eq!(self.made.get(&slice.id), Some(slice), "seed {seed}: the master took a slice no device made");
            assert!(self.taken_slices.insert(slice.id.clone()), "seed {seed}: slice {} was taken twice", slice.id);
            if slice.counts {
                *self.ledger.entry((slice.device_id.clone(), slice.day)).or_default() += slice.secs();
            }
            self.stats.outcome_slices += 1;
        }
        for slice in &report.slices {
            assert!(self.taken_slices.contains(&slice.id), "seed {seed}: slice {} was handed over and not taken", slice.id);
        }
        for piece in &outcome.new_added {
            assert!(self.added.contains_key(&piece.id), "seed {seed}: the master took added time no device added");
            assert!(self.taken_added.insert(piece.id.clone()), "seed {seed}: {} was taken twice", piece.id);
            self.stats.outcome_added += 1;
        }
        for piece in &report.added {
            assert!(self.taken_added.contains(&piece.id), "seed {seed}: {} was handed over and not taken", piece.id);
        }
        for request in &outcome.new_requests {
            assert!(self.taken_requests.insert(request.id.clone()), "seed {seed}: {} was taken twice", request.id);
            self.stats.outcome_requests += 1;
        }
        for request in &report.requests {
            assert!(self.taken_requests.contains(&request.id), "seed {seed}: {} was handed over and not taken", request.id);
        }
        for note in &outcome.new_notes {
            assert_eq!(self.noted.get(&note.id), Some(note), "seed {seed}: the master took a note no device made");
            assert!(self.taken_notes.insert(note.id.clone()), "seed {seed}: note {} was taken twice", note.id);
            self.stats.outcome_notes += 1;
        }
        for note in &report.notes {
            assert!(
                self.taken_notes.contains(&note.id) || self.dropped_notes.contains(&note.id),
                "seed {seed}: note {} was handed over and not taken",
                note.id
            );
        }
        self.check_others(&outcome.grant);
    }

    /// Check 9: a grant's figure of the other devices' use is the master's
    /// own count, for the grant's day, of every device but the one it is
    /// for, removed ones among them; all of it on the master PC exactly when
    /// it was.
    fn check_others(&mut self, grant: &Grant) {
        let (mut others, mut on_master) = (0u64, 0u64);
        for used in self.master.counted_use(grant.day) {
            if used.device_id == grant.device_id {
                continue;
            }
            others += used.counted_secs;
            if !used.removed && self.master.device_view(&used.device_id, grant.day).unwrap().kind == DeviceKind::OnMaster {
                on_master += used.counted_secs;
            }
        }
        assert_eq!(grant.others_used_secs, Some(others), "seed {}: the other devices' use in grant #{}", self.seed, grant.number);
        assert_eq!(grant.others_all_on_master, others > 0 && on_master == others, "seed {}: grant #{}", self.seed, grant.number);
        self.stats.others_checks += 1;
        if grant.others_all_on_master {
            self.stats.others_on_the_pc += 1;
        }
    }

    /// Device `i` takes a grant, and drops the notes it confirms: from any
    /// grant for it, as it drops slices.
    fn take(&mut self, i: usize, grant: &Grant, now: i64) -> GrantOutcome {
        let device = &mut self.devices[i];
        let outcome = device.state.take_grant(grant, now);
        if outcome != GrantOutcome::NotForThisDevice {
            device.notes.retain(|n| !grant.confirmed_notes.contains(&n.id));
        }
        outcome
    }

    /// Now and then a device notes something for the guardian.
    fn maybe_note(&mut self, i: usize, now: i64) {
        if !self.note_rng.per_mille(30) {
            return;
        }
        let rng = &mut self.note_rng;
        let permission = rng.pick(&PhonePermission::ALL);
        let note = match rng.below(5) {
            0 => PhoneNote::Restarted { at: now },
            1 => PhoneNote::PermissionOff { permission, at: now },
            2 => PhoneNote::PermissionOn { permission, at: now },
            3 => PhoneNote::GuardedPageOpened {
                page: rng.pick(&[GuardedPage::DeviceAdmin, GuardedPage::Accessibility]),
                day: self.master.today(now),
                at: now,
            },
            _ => PhoneNote::ClockMoved { by_secs: rng.range(-3_600, 3_600), followed: rng.below(2) == 0, at: now },
        };
        let device = &mut self.devices[i];
        device.last_note += 1;
        let sent = SentNote { id: SentNote::id_for(&device.id, device.last_note), note };
        device.notes.push(sent.clone());
        let over = device.notes.len().saturating_sub(MAX_NOTES_PER_REPORT);
        for dropped in device.notes.drain(..over) {
            self.dropped_notes.insert(dropped.id);
        }
        self.noted.insert(sent.id.clone(), sent);
        self.stats.notes_made += 1;
    }

    /// An old report from a device: handled, and checked, or refused when
    /// the device has been removed, without changing the book.
    fn handle_old_report(&mut self, report: &Report, now: i64) -> Option<(usize, Grant)> {
        let i = self.devices.iter().position(|d| d.id == report.device_id).unwrap();
        if self.devices[i].removed {
            let before = serde_json::to_string(&self.master).unwrap();
            let err = self.master.handle_report(report, now).unwrap_err();
            assert_eq!(err, MasterError::DeviceRemoved { device_id: report.device_id.clone() }, "seed {}", self.seed);
            assert_eq!(serde_json::to_string(&self.master).unwrap(), before, "seed {}: a refused report changed the book", self.seed);
            self.stats.removed_reports_refused += 1;
            return None;
        }
        let outcome = self.master.handle_report(report, now).unwrap();
        self.check_outcome(report, &outcome);
        Some((i, outcome.grant))
    }

    /// The guardian removes a device: now and then right after it reported,
    /// otherwise as it is (a phone lost with use it never reported). Most
    /// often a new device of the same kind is paired in its place.
    fn remove_and_pair(&mut self, now: i64) {
        let live: Vec<usize> = (0..self.devices.len()).filter(|i| !self.devices[*i].removed).collect();
        if live.is_empty() {
            return;
        }
        let i = live[self.removal_rng.below(live.len() as u64) as usize];
        if self.devices[i].in_reach && self.removal_rng.below(2) == 0 {
            self.exchange(i, now, false);
        }
        let id = self.devices[i].id.clone();
        // What it used that the master has not counted, within what the
        // master held back for it, is the most removing it can let a day go
        // over by.
        let mut unreported = 0;
        let days: Vec<(i64, u64)> = self.devices[i].used.iter().map(|(d, s)| (*d, *s)).collect();
        for (day, used) in days {
            let view = self.master.device_view(&id, day).unwrap();
            let missing = used.saturating_sub(view.used_secs);
            let lost = view.reserved_secs.map_or(missing, |held| missing.min(held));
            *self.lost.entry(day).or_default() += lost;
            unreported += lost;
        }
        self.master.remove_device(&id, now).unwrap();
        self.log(|| format!("{now} removed {id}, {unreported} s unreported"));
        assert_eq!(self.master.remove_device(&id, now), Err(MasterError::DeviceRemoved { device_id: id.clone() }));
        if unreported > 0 {
            self.stats.removals_with_unreported_use += 1;
            self.stats.unreported_secs_lost += unreported;
        } else {
            self.stats.removals_after_report += 1;
        }
        let device = &mut self.devices[i];
        device.removed = true;
        device.in_reach = false;
        device.in_use = false;
        self.inbox.retain(|(d, _, _)| *d != i);
        self.used_codes.retain(|(d, _)| *d != i);
        self.late_grants.retain(|(d, _)| *d != i);
        if self.removal_rng.below(4) == 0 {
            return;
        }
        // A new device in its place, under a new id: the removed id is never
        // paired again.
        let kind = if self.devices[i].portable { DeviceKind::Portable } else { DeviceKind::OnMaster };
        assert_eq!(
            self.master.add_device(&id, kind, now).unwrap_err(),
            MasterError::DeviceRemoved { device_id: id.clone() }
        );
        self.paired += 1;
        let new_id = format!("{}-{}", id.split('-').next().unwrap(), self.paired);
        let welcome = self.master.add_device(&new_id, kind, now).unwrap();
        let mut state = DeviceState::from_pairing(&welcome, now);
        if self.setup.ways {
            let secret: Vec<u8> = (0..32).map(|_| self.removal_rng.next() as u8).collect();
            self.master.set_code_secret(&new_id, secret.clone()).unwrap();
            state.set_code_secret(secret.clone()).unwrap();
            self.secrets.push(secret);
        }
        self.log(|| format!("{now} paired {new_id}: share {} cap {}", welcome.away_percent, welcome.away_cap_secs));
        self.devices.push(Device::new(new_id, state, kind == DeviceKind::Portable));
        self.note_held(self.devices.len() - 1, now);
        self.stats.pairings += 1;
    }

    /// A device must never hold more than one lease of unspent lease.
    fn check_one_lease(&self, i: usize, now: i64) {
        let longest = self.history.values().map(|r| r.lease_secs()).max().unwrap();
        let lease_left = self.devices[i].state.holding_at(now).lease_left_secs();
        assert!(lease_left <= longest, "seed {}: {} holds {lease_left} s of lease", self.seed, self.devices[i].id);
    }

    /// Check 3: right after an exchange that went through, a device locked
    /// for lack of time means the master had nothing to hand it.
    fn check_in_reach(&mut self, i: usize, grant_day: i64, d: &Decision) {
        let reason = d.device_lock.map(|l| l.reason);
        if !short_of_time(d) || d.day != grant_day {
            return;
        }
        self.stats.locked_for_time_in_reach += 1;
        // Free time may wait for a device that asked first, and only for it.
        let pool = self.master.pool(d.day);
        let own = self.master.device_view(&self.devices[i].id, d.day).unwrap().waiting_secs;
        let for_others = self.master.unmet_need_secs(d.day) - own;
        assert!(
            pool.free_secs.is_some_and(|free| free <= for_others),
            "seed {}: {} is locked ({reason:?}) right after an exchange, but the master has time free for it: {pool:?}, others wait for {for_others}",
            self.seed,
            self.devices[i].id
        );
        // The master may still hold back an away allowance it has just
        // lowered, until the device acknowledges the lower one with its next
        // report. When the last two grants gave the same allowance, nothing
        // may be held back any more.
        if self.devices[i].clean >= 2 && self.devices[i].same_cap {
            self.stats.strict_checks += 1;
            let left = self.master.time_left_for(&self.devices[i].id, d.day).unwrap();
            assert_eq!(
                left,
                Some(0),
                "seed {}: after two clean exchanges with the same allowance {} is locked but the master says {left:?} s are left for it",
                self.seed,
                self.devices[i].id
            );
        }
    }

    /// Runs one device from `t0` to `t1`: it decides, asks the master when
    /// the decision says so and it can reach it, and uses the device up to
    /// each next check while allowed.
    fn device_step(&mut self, i: usize, t0: i64, t1: i64) {
        let mut t = t0;
        let mut asked_at = None;
        let mut trace = Vec::new();
        for _ in 0..500 {
            let app = self.devices[i].app;
            let d = self.devices[i].state.decide(t, Some(app));
            self.note_held(i, t);
            trace.push(format!(
                "t={t} next={} allowed={} lock={:?} in_hand={:?} left={:?} ask={} in_use={} in_reach={} app={app} holding={:?}",
                d.next_check_at - t,
                d.allowed,
                d.device_lock,
                d.seconds_in_hand,
                d.seconds_left,
                d.ask_master,
                self.devices[i].in_use,
                self.devices[i].in_reach,
                self.devices[i].state.holding_at(t)
            ));
            if d.ask_master && self.devices[i].in_reach && asked_at != Some(t) {
                asked_at = Some(t);
                // A device in use asks for more; an idle one only reports.
                // (A device under a guardian's lock never says ask_master.)
                let want_more = self.devices[i].in_use;
                if let Exchange::Through { grant_day, away_cap, rules_changed, .. } = self.exchange(i, t, want_more) {
                    self.devices[i].count_clean(grant_day, away_cap, rules_changed);
                    let d = self.devices[i].state.decide(t, None);
                    if want_more {
                        self.check_in_reach(i, grant_day, &d);
                    }
                }
                continue;
            }
            assert!(d.next_check_at > t, "seed {}: next check not after now", self.seed);
            assert!(d.next_check_if_idle_at >= d.next_check_at, "seed {}", self.seed);
            let using = d.allowed && self.devices[i].in_use;
            let until = if using { d.next_check_at } else { d.next_check_if_idle_at }.min(t1);
            if using {
                let held_before = self.devices[i].state.holding_at(t);
                let made = self.devices[i].state.record_use(app, t, until);
                self.log(|| format!("{t} {} used {app} until {until}: {:?}", self.devices[i].id,
                    made.iter().map(|s| (s.day, s.secs(), s.counts)).collect::<Vec<_>>()));
                for slice in &made {
                    assert!(self.made.insert(slice.id.clone(), slice.clone()).is_none(), "seed {}: slice id reused", self.seed);
                }
                // Whether a lock is in force is asked of the device's standing,
                // not of the decision being checked.
                if self.guardian_locked(i, t) {
                    // Under a guardian's lock only always allowed apps run,
                    // and they never count.
                    self.stats.lock_checks += 1;
                    assert!(
                        made.iter().all(|s| !s.counts),
                        "seed {}: {} recorded counted use under a guardian's lock: {:?}, standing {:?}, decided again {:?}",
                        self.seed,
                        self.devices[i].id,
                        trace.last(),
                        self.devices[i].state.standing_at(t),
                        self.devices[i].state.clone().decide(t, Some(app)).device_lock
                    );
                }
                for slice in &made {
                    if slice.counts {
                        *self.devices[i].used.entry(slice.day).or_default() += slice.secs();
                    }
                }
                // Use beyond everything the device held: only while an unlock
                // keeps it open. The decision's next check never lies beyond
                // the device's own day, so all of it is on one day.
                let held_now = self.devices[i].state.holding_at(t);
                assert_eq!(held_now.day, held_before.day, "seed {}: use crossed the device's day", self.seed);
                let spent = held_now.used_secs() - held_before.used_secs();
                let beyond = spent.saturating_sub(held_before.in_hand_secs());
                if beyond > 0 && d.seconds_in_hand.is_some() {
                    assert!(
                        d.unlocked_until.is_some(),
                        "seed {}: {} used {beyond} s beyond what it held, with no unlock: {d:?}",
                        self.seed,
                        self.devices[i].id
                    );
                    *self.over.entry((i, held_now.day)).or_default() += beyond;
                    self.stats.over_secs_under_unlock += beyond;
                }
                let held_after = self.devices[i].state.holding_at(until);
                if held_after.day == held_before.day {
                    self.stats.away_use_secs += held_after.away_used_secs - held_before.away_used_secs;
                }
            }
            if until >= t1 {
                return;
            }
            t = until;
        }
        let last: Vec<&String> = trace.iter().rev().take(6).collect();
        panic!("seed {}: device {} looped without moving on:\n{last:#?}", self.seed, self.devices[i].id);
    }

    fn change_rules(&mut self, now: i64) {
        let mut rules = self.master.rules();
        rules.version += 1;
        let today = self.master.today(now);
        match self.rng.below(8) {
            // Today's budget, the change that matters most.
            0 | 1 => rules.budgets.set(Weekday::of_date(today), random_budget(&mut self.rng)),
            2 => rules.budgets.set(Weekday::of_date(today + 1), random_budget(&mut self.rng)),
            3 => rules.away_share_percent = self.rng.pick(&[0, 25, 50, 75, 100]),
            4 => rules.lease_minutes = self.rng.pick(&[1, 5, 10, 15, 30]),
            5 => rules.blocked_hours = (0..self.rng.below(3)).map(|_| random_window(&mut self.rng)).collect(),
            _ if self.setup.calendar_change => {
                if self.rng.below(2) == 0 {
                    let step = self.rng.pick(&[-3_600, 3_600, 1_800]);
                    rules.utc_offset_secs = (rules.utc_offset_secs + step).clamp(-50_400, 50_400);
                } else {
                    rules.day_start_minute = self.rng.pick(&[0, 120, 240, 1_380]);
                }
            }
            _ => rules.budgets.set(Weekday::of_date(today), random_budget(&mut self.rng)),
        }
        self.log(|| format!("{now} rules v{} offset {} day start {} today budget {:?} share {} lease {}",
            rules.version, rules.utc_offset_secs, rules.day_start_minute,
            rules.budget_secs_on(today), rules.away_share_percent, rules.lease_minutes));
        self.master.set_rules(rules.clone()).unwrap();
        self.history.insert(rules.version, rules);
        self.stats.rule_changes += 1;
        for device in &mut self.devices {
            device.clean = 0;
        }
    }

    /// An old report reaches the master again, or an old grant reaches its
    /// device late.
    fn repeat_something(&mut self, now: i64) {
        self.stats.repeats += 1;
        if self.rng.below(2) == 0 && !self.old_reports.is_empty() {
            let report = self.old_reports[self.rng.below(self.old_reports.len() as u64) as usize].clone();
            let Some((i, grant)) = self.handle_old_report(&report, now) else { return };
            self.log(|| format!("{now} repeated report #{} of {}: grant #{} day {} lease {} cap {}",
                report.message_number, report.device_id, grant.number, grant.day, grant.lease_total_secs, grant.away_cap_secs));
            self.in_play.entry(grant.day).or_default().insert(grant.rules.version);
            self.devices[i].clean = 0;
            if self.devices[i].in_reach && self.rng.below(2) == 0 {
                self.take(i, &grant, now);
                self.note_held(i, now);
            }
            self.old_grants.push((i, grant));
        } else if !self.old_grants.is_empty() {
            let (i, grant) = self.old_grants[self.rng.below(self.old_grants.len() as u64) as usize].clone();
            if self.devices[i].in_reach {
                self.devices[i].clean = 0;
                let outcome = self.take(i, &grant, now);
                self.log(|| format!("{now} late grant #{} to {}: {outcome:?}", grant.number, self.devices[i].id));
                if outcome == GrantOutcome::Taken {
                    self.stats.late_grants_taken += 1;
                }
                self.note_held(i, now);
                self.check_one_lease(i, now);
            }
        }
    }

    /// Messages held up on the way arrive now and then, after messages sent
    /// later. A report reaches the master; a grant reaches its device if it is
    /// in reach.
    fn deliver_late(&mut self, now: i64) {
        let reports = std::mem::take(&mut self.late_reports);
        for report in reports {
            if self.rng.below(10) >= 3 {
                self.late_reports.push(report);
                continue;
            }
            let Some((i, grant)) = self.handle_old_report(&report, now) else { continue };
            self.log(|| format!("{now} late report #{} of {}: grant #{} lease {} cap {}",
                report.message_number, report.device_id, grant.number, grant.lease_total_secs, grant.away_cap_secs));
            self.in_play.entry(grant.day).or_default().insert(grant.rules.version);
            self.devices[i].clean = 0;
            self.late_grants.push((i, grant));
        }
        let grants = std::mem::take(&mut self.late_grants);
        for (i, grant) in grants {
            if !self.devices[i].in_reach || self.rng.below(10) >= 3 {
                self.late_grants.push((i, grant));
                continue;
            }
            self.devices[i].clean = 0;
            let outcome = self.take(i, &grant, now);
            self.log(|| format!("{now} late grant #{} to {}: {outcome:?}", grant.number, self.devices[i].id));
            if outcome == GrantOutcome::Taken {
                self.stats.late_grants_taken += 1;
            }
            self.note_held(i, now);
            self.check_one_lease(i, now);
        }
    }

    /// The hub at the end of a step: devices in use that are locked for lack
    /// of time ask again (their retry), then every device the master wants
    /// to hear from, and in reach, reports, until nothing more moves. Then
    /// check 5, if no message was lost on the way.
    fn hub_round(&mut self, now: i64, rules_changed_in_step: bool) {
        self.stats.hub_rounds += 1;
        let mut clean = true;
        for i in 0..self.devices.len() {
            if !self.devices[i].in_reach || !self.devices[i].in_use {
                continue;
            }
            let d = self.devices[i].state.decide(now, None);
            if short_of_time(&d) && matches!(self.exchange(i, now, true), Exchange::Lost) {
                clean = false;
            }
        }
        for _ in 0..20 {
            let mut moved = false;
            for id in self.master.wanted_reports(now) {
                let i = self.devices.iter().position(|d| d.id == id).unwrap();
                if !self.devices[i].in_reach {
                    continue;
                }
                let want_more = self.devices[i].in_use && !self.guardian_locked(i, now);
                if matches!(self.exchange(i, now, want_more), Exchange::Lost) {
                    clean = false;
                }
                moved = true;
            }
            if !moved {
                break;
            }
        }
        if clean && !rules_changed_in_step {
            self.check_given_back(now);
        }
    }

    /// Check 5.
    fn check_given_back(&mut self, now: i64) {
        let day = self.master.today(now);
        if self.master.pool(day).reserved_secs.is_none() {
            return;
        }
        // A device out of reach that waits may hold free time for itself, and
        // the master counts for a few minutes on a phone it has just lowered,
        // which may have left meanwhile.
        let absent_counted_on = self.devices.iter().filter(|d| !d.removed).any(|d| {
            let view = self.master.device_view(&d.id, day).unwrap();
            !d.in_reach && (view.waiting_secs > 0 || view.giving_back_secs > 0)
        });
        if absent_counted_on {
            return;
        }
        for x in 0..self.devices.len() {
            if !self.devices[x].in_reach || !self.devices[x].in_use {
                continue;
            }
            let d = self.devices[x].state.decide(now, None);
            if !short_of_time(&d) || d.day != day {
                continue;
            }
            self.stats.reclaim_checks += 1;
            for p in 0..self.devices.len() {
                let phone = &self.devices[p];
                if p == x || !phone.portable || !phone.in_reach {
                    continue;
                }
                let held = phone.state.holding_at(now);
                let waiting = self.master.device_view(&phone.id, day).unwrap().waiting_secs;
                assert!(
                    held.day != day || held.away_left_secs() == 0 || waiting > 0,
                    "seed {}: {} is in reach and locked ({:?}) after the hub's round, but {} at home still holds {} s of away time",
                    self.seed,
                    self.devices[x].id,
                    d.device_lock,
                    phone.id,
                    held.away_left_secs()
                );
            }
        }
    }

    /// Checks 1 and 2.
    fn check_totals(&mut self) {
        let mut total: BTreeMap<i64, u64> = BTreeMap::new();
        for device in &self.devices {
            for (day, secs) in &device.used {
                *total.entry(*day).or_default() += secs;
            }
        }
        for (day, used) in &total {
            let versions = self.in_play.get(day).cloned().unwrap_or_default();
            assert!(!versions.is_empty(), "seed {}: no rules in play on day {day}", self.seed);
            let mut highest = 0;
            let mut no_limit = false;
            for v in &versions {
                match self.history[v].budget_secs_on(*day) {
                    Some(b) => highest = highest.max(b),
                    None => no_limit = true,
                }
            }
            let extra = self.all_extra(*day);
            let counted_extra = self.master.pool(*day).extra_secs;
            assert!(
                counted_extra <= extra,
                "seed {}: day {day}: the master counts {counted_extra} s of extra time, but only {extra} s were given",
                self.seed
            );
            if no_limit {
                continue;
            }
            let over: u64 = self.over.iter().filter(|((_, d), _)| d == day).map(|(_, s)| *s).sum();
            let lost = self.lost.get(day).copied().unwrap_or(0);
            assert!(
                used - over - lost <= highest + extra,
                "seed {}: day {day}: {used} s counted ({over} s of it beyond what devices held, under unlocks, and {lost} s unreported by removed devices), but the budget is {highest} s and extra {extra} s (rules in play {versions:?}, setup {:?})",
                self.seed,
                self.setup
            );
            if self.setup.rule_change == 0 {
                assert_eq!(versions.len(), 1);
            }
        }
        for (i, device) in self.devices.iter().enumerate() {
            if device.removed {
                // Its counted use stays with the master's days, as the
                // master took it: the sum of the slices it returned.
                for day in device.used.keys() {
                    let counted = self.ledger.get(&(device.id.clone(), *day)).copied().unwrap_or(0);
                    let kept = self.master.counted_use(*day).into_iter().find(|c| c.device_id == device.id);
                    match kept {
                        Some(c) => assert!(c.removed && c.counted_secs == counted, "seed {}: {c:?}, ledger {counted}", self.seed),
                        None => assert_eq!(counted, 0, "seed {}: {}'s use on day {day} was dropped", self.seed, device.id),
                    }
                    self.stats.removed_count_checks += 1;
                }
                continue;
            }
            for (day, secs) in &device.used {
                let view = self.master.device_view(&device.id, *day).unwrap();
                // Check 8: the master counts exactly the slices it returned.
                let counted = self.ledger.get(&(device.id.clone(), *day)).copied().unwrap_or(0);
                assert_eq!(
                    view.used_secs, counted,
                    "seed {}: {} on day {day}: the master counts {} s, the slices it returned {counted} s",
                    self.seed, device.id, view.used_secs
                );
                self.stats.ledger_checks += 1;
                let over = self.over.get(&(i, *day)).copied().unwrap_or(0);
                // Time added on the device that the master has not heard of
                // yet is extra it cannot hold back.
                let known: BTreeSet<String> = self.master.extra_time(*day).into_iter().map(|e| e.id).collect();
                let prefix = format!("{}/", device.id);
                let unknown: u64 = self
                    .added
                    .iter()
                    .filter(|(id, (d, _))| d == day && id.starts_with(&prefix) && !known.contains(*id))
                    .map(|(_, (_, s))| *s)
                    .sum();
                if let Some(reserved) = view.reserved_secs {
                    assert!(
                        view.used_secs + reserved + over + unknown >= *secs,
                        "seed {}: {} used {secs} s on day {day}, but the master counts {} s and holds back {reserved} s (and {over} s were used beyond what it held, {unknown} s added on it unheard of)",
                        self.seed,
                        device.id,
                        view.used_secs
                    );
                }
                // The master never counts less than the device handed over,
                // unlock or not: what it counts plus what is still on the way
                // is all of it.
                let on_the_way: u64 = device
                    .state
                    .unconfirmed_slices()
                    .iter()
                    .filter(|s| s.day == *day && s.counts)
                    .map(|s| s.secs())
                    .sum();
                assert!(
                    view.used_secs + on_the_way >= *secs,
                    "seed {}: {} used {secs} s on day {day}, the master counts {} s and {on_the_way} s are on the way",
                    self.seed,
                    device.id,
                    view.used_secs
                );
                self.stats.count_checks += 1;
            }
        }
    }

    /// All extra time for a day: given at the master, and added on devices.
    fn all_extra(&self, day: i64) -> u64 {
        let given = self.extra_given.get(&day).copied().unwrap_or(0);
        let added: u64 = self.added.values().filter(|(d, _)| *d == day).map(|(_, s)| *s).sum();
        given + added
    }

    fn guardian_locked(&self, i: usize, now: i64) -> bool {
        self.devices[i]
            .state
            .standing_at(now)
            .override_at(now)
            .is_some_and(|o| o.kind == limo_core::OverrideKind::Lock)
    }

    /// What the guardian does at the master in one step.
    fn guardian_step(&mut self, now: i64) {
        let today = self.master.today(now);
        // Extra time with an id, sometimes sent twice.
        if self.rng.per_mille(3) {
            let minutes = self.rng.pick(&[5, 15, 30]);
            let id = if !self.given_ids.is_empty() && self.rng.below(3) == 0 {
                self.given_ids[self.rng.below(self.given_ids.len() as u64) as usize].clone()
            } else {
                format!("given-{}", self.given_ids.len())
            };
            if self.master.give_extra(&id, minutes, now).unwrap() {
                *self.extra_given.entry(today).or_default() += minutes as u64 * 60;
                self.stats.extra_given += 1;
                self.given_ids.push(id);
            } else {
                self.stats.extra_repeats += 1;
            }
        }
        // A code read out over a call, typed now or a few hours later.
        if self.rng.per_mille(6) {
            let i = self.rng.below(self.devices.len() as u64) as usize;
            let minutes = self.rng.pick(&[15, 30, 60]);
            let made = self.counters.entry((i, today)).or_default().clone();
            // No code is made for a removed device: its secret is gone.
            let code = if self.devices[i].removed {
                None
            } else {
                Some(next_code(self.secrets[i].clone(), &self.devices[i].id, today, minutes, made))
            };
            match code {
                Some(Ok(code)) => {
                    self.counters.entry((i, today)).or_default().push(code.counter);
                    let typed_at = now + if self.rng.below(5) == 0 { self.rng.range(0, 6 * 3_600) } else { 0 };
                    self.inbox.push((i, code, typed_at));
                    self.stats.codes_made += 1;
                }
                Some(Err(_)) => self.stats.codes_exhausted += 1,
                None => {}
            }
        }
        // The PIN changes now and then.
        if self.rng.per_mille(1) {
            self.pin = random_pin(&mut self.rng);
            self.master.set_pin_record(quick_pin(&self.pin)).unwrap();
            self.stats.pin_changes += 1;
        }
        // A lock or an unlock from the master, or lifting it.
        let lock_until_lifted = matches!(
            self.master.guardian_setting().mode,
            limo_core::GuardianMode::Lock { until: None }
        );
        if self.rng.per_mille(3) || (lock_until_lifted && self.rng.per_mille(30)) {
            match self.rng.below(20) {
                _ if lock_until_lifted => {
                    self.master.lift(now);
                    self.stats.lifts += 1;
                }
                0..=6 => {
                    self.master.lock_now(Some(now + self.rng.range(600, 5_400)), now).unwrap();
                    self.stats.guardian_locks += 1;
                }
                7 => {
                    self.master.lock_now(None, now).unwrap();
                    self.stats.guardian_locks += 1;
                }
                8..=15 => {
                    self.master.unlock_until(now + self.rng.range(600, 7_200), now).unwrap();
                    self.stats.guardian_unlocks += 1;
                }
                _ => {
                    self.master.lift(now);
                    self.stats.lifts += 1;
                }
            }
            self.log(|| format!("{now} guardian setting {:?}", self.master.guardian_setting()));
        }
        // Answers to the requests waiting today.
        for view in self.master.waiting_requests(now) {
            if !self.rng.per_mille(250) {
                continue;
            }
            if self.rng.below(3) == 0 {
                self.master.answer_request(&view.request.id, RequestReply::Refuse, now).unwrap();
                self.stats.requests_refused += 1;
            } else {
                let minutes = self.rng.pick(&[5, 10, 20, 30]);
                let answered =
                    self.master.answer_request(&view.request.id, RequestReply::Grant { minutes }, now).unwrap();
                assert_eq!(answered.answer, RequestAnswer::Granted { minutes });
                *self.extra_given.entry(view.request.day).or_default() += minutes as u64 * 60;
                self.stats.requests_granted += 1;
            }
        }
    }

    /// Notes what a code or the PIN did on device `i`.
    fn note_try(&mut self, i: usize, outcome: &TryOutcome) {
        self.log(|| format!("{} tried a code or the PIN: {outcome:?}", self.devices[i].id));
        match outcome {
            TryOutcome::Added { added } => {
                let fresh = self.added.insert(added.id.clone(), (added.day, added.secs)).is_none();
                assert!(fresh, "seed {}: the id {} was added twice", self.seed, added.id);
                let away = !self.devices[i].in_reach;
                match (added.id.contains("/code/"), away) {
                    (true, false) => self.stats.codes_used_in_reach += 1,
                    (true, true) => self.stats.codes_used_away += 1,
                    (false, false) => self.stats.pin_adds_in_reach += 1,
                    (false, true) => self.stats.pin_adds_away += 1,
                }
            }
            TryOutcome::MustWait { .. } => self.stats.waits_hit += 1,
            TryOutcome::AlreadyUsed => self.stats.codes_reused += 1,
            _ => {}
        }
    }

    /// The ways out on device `i` at `now`: codes typed, the PIN typed, a
    /// restart.
    fn device_ways(&mut self, i: usize, now: i64) {
        // Codes read out to this device whose moment has come.
        let mut due = Vec::new();
        self.inbox.retain(|(device, code, typed_at)| {
            if *device == i && *typed_at <= now {
                due.push(code.clone());
                false
            } else {
                true
            }
        });
        for code in due {
            // Now and then the child types it on another device first.
            let other = (i + 1) % self.devices.len();
            if self.devices.len() > 1 && self.rng.below(10) == 0 && !self.devices[other].removed {
                let outcome = self.devices[other].state.try_code(&code.shown, now);
                assert!(
                    matches!(outcome, TryOutcome::Wrong { .. } | TryOutcome::MustWait { .. }),
                    "seed {}: a code for {} worked on {}: {outcome:?}",
                    self.seed,
                    self.devices[i].id,
                    self.devices[other].id
                );
                self.note_try(other, &outcome);
                self.stats.codes_on_wrong_device += 1;
            }
            let device_day = self.devices[i].state.holding_at(now).day;
            let outcome = self.devices[i].state.try_code(&code.shown, now);
            if device_day != code.day {
                // A code for another day is simply wrong.
                assert!(
                    matches!(outcome, TryOutcome::Wrong { .. } | TryOutcome::MustWait { .. }),
                    "seed {}: a code for day {} worked on day {device_day}: {outcome:?}",
                    self.seed,
                    code.day
                );
                self.stats.codes_old += 1;
            }
            self.note_try(i, &outcome);
            if matches!(outcome, TryOutcome::Added { .. }) {
                self.used_codes.push((i, code.digits.clone()));
            }
        }
        // A code that worked, typed again.
        if !self.used_codes.is_empty() && self.rng.per_mille(3) {
            let (device, digits) = self.used_codes[self.rng.below(self.used_codes.len() as u64) as usize].clone();
            if device == i {
                let outcome = self.devices[i].state.try_code(&digits, now);
                assert!(
                    !matches!(outcome, TryOutcome::Added { .. }),
                    "seed {}: a used code added time again",
                    self.seed
                );
                self.note_try(i, &outcome);
            }
        }
        // A child guesses a code, now and then several in a row.
        if self.rng.per_mille(4) {
            let tries = if self.rng.below(4) == 0 { 7 } else { 1 };
            for _ in 0..tries {
                let guess = format!("{:08}", self.rng.below(100_000_000));
                let outcome = self.devices[i].state.try_code(&guess, now);
                self.note_try(i, &outcome);
                self.stats.random_codes += 1;
            }
        }
        // A guardian with the PIN at the device: more time, or an unlock;
        // now and then a slip of the finger.
        if self.rng.per_mille(3) {
            let pin = if self.rng.below(8) == 0 { "0000".to_string() } else { self.pin.clone() };
            let action = if self.rng.below(5) < 3 {
                PinAction::AddMinutes { minutes: self.rng.pick(&[5, 15, 30, 45]) }
            } else {
                PinAction::UnlockUntil { until: now + self.rng.range(600, 5_400) }
            };
            let outcome = self.devices[i].state.try_pin(&pin, action, now).unwrap();
            match &outcome {
                TryOutcome::Unlocked { .. } => self.stats.pin_unlocks += 1,
                TryOutcome::Wrong { .. } => self.stats.pin_wrong += 1,
                _ => {}
            }
            self.note_try(i, &outcome);
        }
        // The device restarts from its saved state.
        if self.rng.per_mille(5) {
            let saved = serde_json::to_string(&self.devices[i].state).unwrap();
            let loaded: DeviceState = serde_json::from_str(&saved).unwrap();
            assert_eq!(loaded, self.devices[i].state, "seed {}: state changed when saved and loaded", self.seed);
            self.devices[i].state = loaded;
            self.stats.restarts += 1;
        }
    }

    /// A child at a device locked for lack of time asks for more, now and
    /// then.
    fn maybe_ask(&mut self, i: usize, now: i64) {
        if !self.devices[i].in_use {
            return;
        }
        let d = self.devices[i].state.decide(now, None);
        if short_of_time(&d) && d.can_ask_for_time && self.rng.per_mille(150) {
            let minutes = self.rng.pick(&[10, 15, 30, 60]);
            self.devices[i].state.ask_for_time(minutes, "", now).unwrap();
            self.stats.requests_made += 1;
        }
    }

    fn run(&mut self, start: i64) {
        let end = start + self.setup.days * 86_400;
        let mut now = start;
        while now < end {
            let step = self.rng.range(20, 300);
            self.stats.steps += 1;
            let rules_changed = self.rng.per_mille(self.setup.rule_change);
            if rules_changed {
                self.change_rules(now);
            }
            if self.rng.per_mille(self.setup.extra) {
                let day = self.master.today(now);
                let secs = self.rng.pick(&[300, 900, 1_800]);
                self.master.add_extra_secs(day, secs);
                *self.extra_given.entry(day).or_default() += secs;
            }
            if self.setup.ways {
                self.guardian_step(now);
            }
            if self.rng.per_mille(self.setup.repeat) {
                self.repeat_something(now);
            }
            if self.setup.removal > 0 && self.removal_rng.per_mille(self.setup.removal) {
                self.remove_and_pair(now);
            }
            self.deliver_late(now);
            for i in 0..self.devices.len() {
                if self.devices[i].removed {
                    continue;
                }
                let rng = &mut self.rng;
                let device = &mut self.devices[i];
                if device.portable && rng.per_mille(self.setup.reach_change) {
                    device.in_reach = !device.in_reach;
                }
                if rng.per_mille(150) {
                    device.in_use = !device.in_use;
                }
                if rng.per_mille(100) {
                    device.app = rng.pick(&APPS);
                }
                if device.portable && !device.in_reach {
                    self.stats.phone_minutes_away += step as u64 / 60;
                }
                if self.setup.ways {
                    self.device_ways(i, now);
                }
                self.maybe_note(i, now);
                // Now and then a device in reach reports without being asked.
                if self.devices[i].in_reach && self.rng.per_mille(100) {
                    let want_more = self.devices[i].in_use && !self.guardian_locked(i, now);
                    if let Exchange::Through { grant_day, away_cap, rules_changed, .. } =
                        self.exchange(i, now, want_more)
                    {
                        self.devices[i].count_clean(grant_day, away_cap, rules_changed);
                    }
                }
                self.device_step(i, now, now + step);
                if self.setup.ways {
                    self.maybe_ask(i, now + step);
                }
            }
            if self.setup.hub {
                self.hub_round(now + step, rules_changed);
            }
            self.check_totals();
            now += step;
        }
        // How many days reached their limit, so that the property is known
        // to have been tested where it matters.
        let mut total: BTreeMap<i64, u64> = BTreeMap::new();
        for device in &self.devices {
            for (day, secs) in &device.used {
                *total.entry(*day).or_default() += secs;
            }
        }
        for (day, used) in total {
            self.stats.days_checked += 1;
            if let Some(budget) = self.master.rules().budget_secs_on(day)
                && budget > 0
                && used + 60 >= budget + self.all_extra(day)
            {
                self.stats.days_at_the_limit += 1;
            }
        }
        self.stats.scenarios = 1;
    }
}

/// A PIN of 6 to 8 digits.
fn random_pin(rng: &mut Rng) -> String {
    let digits = rng.range(6, 8) as usize;
    (0..digits).map(|_| char::from(b'0' + rng.below(10) as u8)).collect()
}

/// A PIN record that is quick to check: the simulation tries thousands.
fn quick_pin(pin: &str) -> PinRecord {
    PinRecord::with_iterations(pin, (0u8..16).collect(), 1).unwrap()
}

/// Locked for lack of time, not for blocked hours or a blocked app.
fn short_of_time(d: &Decision) -> bool {
    matches!(
        d.device_lock.map(|l| l.reason),
        Some(
            LockReason::TimeUsedUp
                | LockReason::NeedsMaster
                | LockReason::NoTimeToday
                | LockReason::HeldByAnotherDevice
        )
    )
}

fn scenario_count() -> u64 {
    std::env::var("LIMO_SIM_SCENARIOS").ok().and_then(|v| v.parse().ok()).unwrap_or(300)
}

/// The seeds to run: every scenario, or only the one in `LIMO_SIM_SEED`.
fn seeds() -> Vec<u64> {
    if let Some(seed) = std::env::var("LIMO_SIM_SEED").ok().and_then(|v| v.parse().ok()) {
        return vec![seed];
    }
    (0..scenario_count()).map(|n: u64| n.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0x1D1D).collect()
}

#[test]
fn many_generated_days_never_go_over_the_limit() {
    let mut all = Stats::default();
    let seeds = seeds();
    let single = seeds.len() == 1;
    for seed in seeds {
        let world = World::new(seed);
        let s = world.stats;
        all.scenarios += s.scenarios;
        all.steps += s.steps;
        all.exchanges += s.exchanges;
        all.lost += s.lost;
        all.repeats += s.repeats;
        all.held_up += s.held_up;
        all.late_grants_taken += s.late_grants_taken;
        all.rule_changes += s.rule_changes;
        all.days_checked += s.days_checked;
        all.days_at_the_limit += s.days_at_the_limit;
        all.locked_for_time_in_reach += s.locked_for_time_in_reach;
        all.strict_checks += s.strict_checks;
        all.phone_minutes_away += s.phone_minutes_away;
        all.away_use_secs += s.away_use_secs;
        all.hub_rounds += s.hub_rounds;
        all.reclaims += s.reclaims;
        all.reclaimed_secs += s.reclaimed_secs;
        all.reclaim_checks += s.reclaim_checks;
        all.extra_given += s.extra_given;
        all.extra_repeats += s.extra_repeats;
        all.codes_made += s.codes_made;
        all.codes_exhausted += s.codes_exhausted;
        all.codes_used_in_reach += s.codes_used_in_reach;
        all.codes_used_away += s.codes_used_away;
        all.codes_reused += s.codes_reused;
        all.codes_old += s.codes_old;
        all.codes_on_wrong_device += s.codes_on_wrong_device;
        all.random_codes += s.random_codes;
        all.pin_adds_in_reach += s.pin_adds_in_reach;
        all.pin_adds_away += s.pin_adds_away;
        all.pin_unlocks += s.pin_unlocks;
        all.pin_wrong += s.pin_wrong;
        all.pin_changes += s.pin_changes;
        all.waits_hit += s.waits_hit;
        all.guardian_locks += s.guardian_locks;
        all.guardian_unlocks += s.guardian_unlocks;
        all.lifts += s.lifts;
        all.requests_made += s.requests_made;
        all.requests_granted += s.requests_granted;
        all.requests_refused += s.requests_refused;
        all.over_secs_under_unlock += s.over_secs_under_unlock;
        all.lock_checks += s.lock_checks;
        all.restarts += s.restarts;
        all.count_checks += s.count_checks;
        all.removals_after_report += s.removals_after_report;
        all.removals_with_unreported_use += s.removals_with_unreported_use;
        all.unreported_secs_lost += s.unreported_secs_lost;
        all.pairings += s.pairings;
        all.removed_reports_refused += s.removed_reports_refused;
        all.removed_count_checks += s.removed_count_checks;
        all.outcome_slices += s.outcome_slices;
        all.outcome_added += s.outcome_added;
        all.outcome_requests += s.outcome_requests;
        all.ledger_checks += s.ledger_checks;
        all.notes_made += s.notes_made;
        all.outcome_notes += s.outcome_notes;
        all.others_checks += s.others_checks;
        all.others_on_the_pc += s.others_on_the_pc;
    }
    println!("{all:#?}");
    if single {
        return;
    }
    // The scenarios must really reach the limits, lose messages and so on,
    // or passing would prove little.
    assert!(all.days_at_the_limit * 4 >= all.days_checked, "{all:#?}");
    assert!(all.lost > 0 && all.repeats > 0 && all.late_grants_taken > 0 && all.rule_changes > 0, "{all:#?}");
    assert!(all.locked_for_time_in_reach > 0 && all.strict_checks > 0 && all.away_use_secs > 0, "{all:#?}");
    assert!(all.reclaims > 0 && all.reclaimed_secs > 0 && all.reclaim_checks > 0, "{all:#?}");
    // Every way out must be taken, or the run proves nothing about it.
    let paths = [
        ("extra time given at the master", all.extra_given),
        ("the same extra time given twice", all.extra_repeats),
        ("a code used in reach", all.codes_used_in_reach),
        ("a code used out of reach", all.codes_used_away),
        ("a used code typed again", all.codes_reused),
        ("a code typed on a later day", all.codes_old),
        ("a code typed on the wrong device", all.codes_on_wrong_device),
        ("time added with the PIN in reach", all.pin_adds_in_reach),
        ("time added with the PIN out of reach", all.pin_adds_away),
        ("an unlock with the PIN", all.pin_unlocks),
        ("a wrong PIN", all.pin_wrong),
        ("a changed PIN", all.pin_changes),
        ("a wait after wrong guesses", all.waits_hit),
        ("a lock from the master", all.guardian_locks),
        ("an unlock from the master", all.guardian_unlocks),
        ("a lift at the master", all.lifts),
        ("a request made", all.requests_made),
        ("a request granted", all.requests_granted),
        ("a request refused", all.requests_refused),
        ("use beyond what a device held, under an unlock", all.over_secs_under_unlock),
        ("a device in use under a guardian's lock", all.lock_checks),
        ("a restart from saved state", all.restarts),
        ("a device removed right after its report", all.removals_after_report),
        ("a device removed with use it had not reported", all.removals_with_unreported_use),
        ("a new device paired in a removed one's place", all.pairings),
        ("a late report from a removed device, refused", all.removed_reports_refused),
        ("a removed device's use checked on its days", all.removed_count_checks),
        ("new slices in a report's outcome", all.outcome_slices),
        ("new added time in a report's outcome", all.outcome_added),
        ("new requests in a report's outcome", all.outcome_requests),
        ("the master's count against the slices it returned", all.ledger_checks),
        ("new notes in a report's outcome", all.outcome_notes),
        ("the other devices' use in a grant, checked", all.others_checks),
        ("all of the other devices' use on the master PC", all.others_on_the_pc),
    ];
    for (path, count) in paths {
        assert!(count > 0, "the scenarios never took this path: {path}\n{all:#?}");
    }
}
