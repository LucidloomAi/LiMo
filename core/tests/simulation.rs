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
//!    device covers everything the device really used.
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
//! The run fails if the scenarios never take time back from a phone, or never
//! reach the moment where check 5 applies, so that it cannot pass without
//! testing the path.

use std::collections::{BTreeMap, BTreeSet};

use limo_core::{
    BlockedWindow, DailyBudget, Decision, DeviceKind, DeviceState, Grant, GrantOutcome, LockReason,
    MasterBook, Report, RuleSet, Weekday,
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
}

struct Device {
    id: String,
    state: DeviceState,
    portable: bool,
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
}

impl Device {
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
        };
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
            devices.push(Device {
                id,
                state: DeviceState::from_pairing(&welcome, start),
                portable: kind == DeviceKind::Portable,
                in_reach: true,
                in_use: false,
                app: "game",
                used: BTreeMap::new(),
                clean: 0,
                last_cap: None,
                same_cap: false,
            });
        }
        let mut history = BTreeMap::new();
        history.insert(1, rules);
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
        let report = self.devices[i].state.report(now, want_more);
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
        let grant = self.master.handle_report(&report, now).unwrap();
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
        let outcome = self.devices[i].state.take_grant(&grant, now);
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
                for slice in made {
                    if slice.counts {
                        *self.devices[i].used.entry(slice.day).or_default() += slice.secs();
                    }
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
            let grant = self.master.handle_report(&report, now).unwrap();
            self.log(|| format!("{now} repeated report #{} of {}: grant #{} day {} lease {} cap {}",
                report.message_number, report.device_id, grant.number, grant.day, grant.lease_total_secs, grant.away_cap_secs));
            self.in_play.entry(grant.day).or_default().insert(grant.rules.version);
            let i = self.devices.iter().position(|d| d.id == report.device_id).unwrap();
            self.devices[i].clean = 0;
            if self.devices[i].in_reach && self.rng.below(2) == 0 {
                self.devices[i].state.take_grant(&grant, now);
                self.note_held(i, now);
            }
            self.old_grants.push((i, grant));
        } else if !self.old_grants.is_empty() {
            let (i, grant) = self.old_grants[self.rng.below(self.old_grants.len() as u64) as usize].clone();
            if self.devices[i].in_reach {
                self.devices[i].clean = 0;
                let outcome = self.devices[i].state.take_grant(&grant, now);
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
            let grant = self.master.handle_report(&report, now).unwrap();
            self.log(|| format!("{now} late report #{} of {}: grant #{} lease {} cap {}",
                report.message_number, report.device_id, grant.number, grant.lease_total_secs, grant.away_cap_secs));
            self.in_play.entry(grant.day).or_default().insert(grant.rules.version);
            let i = self.devices.iter().position(|d| d.id == report.device_id).unwrap();
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
            let outcome = self.devices[i].state.take_grant(&grant, now);
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
                let want_more = self.devices[i].in_use;
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
        let absent_counted_on = self.devices.iter().any(|d| {
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
            if no_limit {
                continue;
            }
            let extra = self.master.pool(*day).extra_secs;
            assert!(
                *used <= highest + extra,
                "seed {}: day {day}: {used} s counted, but the budget is {highest} s and extra {extra} s (rules in play {versions:?}, setup {:?})",
                self.seed,
                self.setup
            );
            if self.setup.rule_change == 0 {
                assert_eq!(versions.len(), 1);
            }
        }
        for device in &self.devices {
            for (day, secs) in &device.used {
                let view = self.master.device_view(&device.id, *day).unwrap();
                if let Some(reserved) = view.reserved_secs {
                    assert!(
                        view.used_secs + reserved >= *secs,
                        "seed {}: {} used {secs} s on day {day}, but the master counts {} s and holds back {reserved} s",
                        self.seed,
                        device.id,
                        view.used_secs
                    );
                }
            }
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
                self.master.add_extra_secs(day, self.rng.pick(&[300, 900, 1_800]));
            }
            if self.rng.per_mille(self.setup.repeat) {
                self.repeat_something(now);
            }
            self.deliver_late(now);
            for i in 0..self.devices.len() {
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
                // Now and then a device in reach reports without being asked.
                if self.devices[i].in_reach && self.rng.per_mille(100) {
                    let want_more = self.devices[i].in_use;
                    if let Exchange::Through { grant_day, away_cap, rules_changed, .. } =
                        self.exchange(i, now, want_more)
                    {
                        self.devices[i].count_clean(grant_day, away_cap, rules_changed);
                    }
                }
                self.device_step(i, now, now + step);
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
            if let Some(budget) = self.master.rules().budget_secs_on(day) {
                if budget > 0 && used + 60 >= budget + self.master.pool(day).extra_secs {
                    self.stats.days_at_the_limit += 1;
                }
            }
        }
        self.stats.scenarios = 1;
    }
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
}
