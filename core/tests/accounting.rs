//! The shared budget, told as short stories with a master, a PC and a phone.
//! Each test is one situation from the README, played step by step.
//!
//! A small hub plays the master PC's service: after every exchange it asks
//! the devices the master wants to hear from, and can reach, to report at
//! once ([`MasterBook::wanted_reports`]).

use limo_core::{
    CivilDate, DailyBudget, Decision, DeviceKind, DeviceState, Grant, GrantOutcome, LockReason,
    MasterBook, Report, RuleSet, WeekBudgets, Weekday,
};

const MIN: u64 = 60;
const INDIA: i32 = 19_800;

/// Home time in India, November 2026 (the 18th is a Wednesday).
fn at(day: u8, hour: u8, minute: u8) -> i64 {
    RuleSet::new(1, INDIA).calendar().at(CivilDate::new(2026, 11, day).unwrap(), hour, minute, 0)
}

fn rules(minutes: u32) -> RuleSet {
    let mut r = RuleSet::new(1, INDIA);
    r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(minutes));
    r.always_allowed_apps = vec!["clock".into()];
    r
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Who {
    Pc,
    Phone,
}

struct Home {
    master: MasterBook,
    pc: DeviceState,
    phone: DeviceState,
    /// The phone is at home, in reach of the master.
    phone_home: bool,
    /// The hub asks the devices the master wants to hear from to report.
    hub_on: bool,
}

fn home(rules: RuleSet, now: i64) -> Home {
    let mut master = MasterBook::new("asha", rules).unwrap();
    let pc = DeviceState::from_pairing(&master.add_device("pc", DeviceKind::OnMaster, now).unwrap(), now);
    let phone = DeviceState::from_pairing(&master.add_device("phone", DeviceKind::Portable, now).unwrap(), now);
    Home { master, pc, phone, phone_home: true, hub_on: true }
}

impl Home {
    fn device(&mut self, who: Who) -> &mut DeviceState {
        match who {
            Who::Pc => &mut self.pc,
            Who::Phone => &mut self.phone,
        }
    }

    fn reachable(&self, who: Who) -> bool {
        who == Who::Pc || self.phone_home
    }

    /// One report and its grant, both delivered. A grant that asks to be
    /// acknowledged at once is, with a report that asks for more if the first
    /// one did. Returns the first grant.
    fn exchange(&mut self, who: Who, now: i64, want_more: bool) -> Grant {
        let report = self.device(who).report(now, want_more);
        let first = self.master.handle_report(&report, now).unwrap();
        assert_eq!(self.device(who).take_grant(&first, now), GrantOutcome::Taken);
        let mut grant = first.clone();
        for _ in 0..3 {
            if !grant.acknowledge_now {
                break;
            }
            let report = self.device(who).report(now, want_more);
            grant = self.master.handle_report(&report, now).unwrap();
            assert_eq!(self.device(who).take_grant(&grant, now), GrantOutcome::Taken);
        }
        first
    }

    /// The hub: asks every device the master wants to hear from, and can
    /// reach, to report, until nothing more moves. A device that is waiting
    /// for time asks for more; the others only report.
    fn hub(&mut self, now: i64) {
        if !self.hub_on {
            return;
        }
        for _ in 0..20 {
            let day = self.master.today(now);
            let mut moved = false;
            for id in self.master.wanted_reports(now) {
                let who = if id == "pc" { Who::Pc } else { Who::Phone };
                if !self.reachable(who) {
                    continue;
                }
                let waiting = self.master.device_view(&id, day).unwrap().waiting_secs > 0;
                self.exchange(who, now, waiting);
                moved = true;
            }
            if !moved {
                return;
            }
        }
        panic!("the hub kept moving time without settling");
    }

    /// A device stops being used and says so: it reports without asking for
    /// more, so that it is no longer waiting for anything.
    fn idle(&mut self, who: Who, now: i64) {
        if self.reachable(who) {
            self.exchange(who, now, false);
            self.hub(now);
        }
    }

    /// Uses a device from `from` until `to`, asking the master whenever the
    /// decision says so and the master is in reach. While it is locked for
    /// lack of time the child waits at the lock screen and the device asks
    /// again every minute, as the decision tells it to. Returns the counted
    /// seconds used and the last decision.
    fn play(&mut self, who: Who, from: i64, to: i64) -> (u64, Decision) {
        let mut t = from;
        let mut used = 0;
        loop {
            let mut d = self.device(who).decide(t, Some("game"));
            if d.ask_master {
                if self.reachable(who) {
                    self.exchange(who, t, true);
                    self.hub(t);
                    d = self.device(who).decide(t, Some("game"));
                } else {
                    self.device(who).lost_contact();
                }
            }
            if t >= to {
                return (used, d);
            }
            if !d.allowed {
                if reason(&d) == Some(LockReason::BlockedHours) {
                    return (used, d);
                }
                t = d.next_check_if_idle_at.min(to);
                continue;
            }
            let until = d.next_check_at.min(to);
            used += self.device(who).record_use("game", t, until).iter().filter(|s| s.counts).map(|s| s.secs()).sum::<u64>();
            t = until;
        }
    }

    /// The master's view of a day in minutes: used, held back, free.
    fn pool(&self, day: i64) -> (u64, u64, u64) {
        let p = self.master.pool(day);
        (p.used_secs / MIN, p.reserved_secs.unwrap() / MIN, p.free_secs.unwrap() / MIN)
    }
}

fn reason(d: &Decision) -> Option<LockReason> {
    d.device_lock.map(|l| l.reason)
}

#[test]
fn the_worked_day_in_the_readme() {
    // Two hours a day, half of it as the phone's away share, ten minute
    // leases. Paired on Tuesday evening; the story is Wednesday.
    let mut h = home(rules(120), at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    // 00:00. A new day. The phone works out its away allowance alone, and
    // the master holds back the same amount for it.
    assert_eq!(h.phone.holding_at(at(18, 0, 0)).away_left_secs(), 60 * MIN);
    assert_eq!(h.pool(wed), (0, 60, 60));
    // 07:30. The phone, at home, asks for a lease.
    h.exchange(Who::Phone, at(18, 7, 30), true);
    assert_eq!(h.pool(wed), (0, 70, 50));
    // 10:00 to 10:45. At school, out of reach: the lease first, then 35
    // minutes of the away allowance. 25 minutes of it are left.
    h.phone_home = false;
    let (school, _) = h.play(Who::Phone, at(18, 10, 0), at(18, 10, 45));
    assert_eq!(school, 45 * MIN);
    assert_eq!(h.phone.holding_at(at(18, 10, 45)).away_left_secs(), 25 * MIN);
    assert_eq!(h.pool(wed), (0, 70, 50), "the master has heard nothing yet");
    // 16:00. The PC at home takes leases from the 50 free minutes and locks
    // at 16:50. The rest is held for the phone, which is not home to give it
    // back, so the PC waits.
    let (pc, d) = h.play(Who::Pc, at(18, 16, 0), at(18, 17, 0));
    assert_eq!(pc, 50 * MIN);
    assert_eq!(reason(&d), Some(LockReason::HeldByAnotherDevice));
    assert_eq!(h.pool(wed), (50, 70, 0));
    assert_eq!(h.master.wanted_reports(at(18, 17, 0)), vec!["phone".to_string()]);
    // 17:00. The phone comes home and reports its 45 minutes. The PC is
    // waiting for 10, so the phone's away allowance is lowered by 10, it
    // acknowledges at once, and the PC gets its lease.
    h.phone_home = true;
    h.exchange(Who::Phone, at(18, 17, 0), false);
    assert_eq!(h.pool(wed), (95, 15, 10));
    h.hub(at(18, 17, 0));
    assert_eq!(h.pool(wed), (95, 25, 0));
    // 17:00 to 17:25. The PC goes on, and the phone at home gives back the
    // rest of its away allowance as the PC needs it.
    let (evening, d) = h.play(Who::Pc, at(18, 17, 0), at(18, 18, 0));
    assert_eq!(evening, 25 * MIN);
    assert_eq!(reason(&d), Some(LockReason::TimeUsedUp));
    assert_eq!(h.pool(wed), (120, 0, 0));
    assert_eq!(h.phone.holding_at(at(18, 17, 30)).away_left_secs(), 0);
    // 19:00. The phone has nothing left: the PC used it.
    let (late, d) = h.play(Who::Phone, at(18, 19, 0), at(18, 20, 0));
    assert_eq!(late, 0);
    assert_eq!(reason(&d), Some(LockReason::TimeUsedUp));
    assert_eq!(school + pc + evening, 120 * MIN);
}

#[test]
fn with_the_phone_lying_at_home_the_pc_gets_the_whole_budget() {
    // Story (a). The phone is paired, at home and unused all day.
    let mut h = home(rules(120), at(18, 7, 0));
    let today = h.master.today(at(18, 7, 0));
    let (pc, d) = h.play(Who::Pc, at(18, 9, 0), at(18, 23, 0));
    assert_eq!(pc, 120 * MIN);
    assert_eq!(reason(&d), Some(LockReason::TimeUsedUp));
    assert_eq!(h.pool(today), (120, 0, 0));
    assert_eq!(h.phone.holding_at(at(18, 23, 0)).away_cap_secs, 0);
}

#[test]
fn the_pc_gets_all_but_what_the_phone_used_and_still_holds_then_the_phone_leaves() {
    // Story (a) with morning use on the phone, then story (b).
    let mut h = home(rules(120), at(18, 7, 0));
    let today = h.master.today(at(18, 7, 0));
    let (morning, _) = h.play(Who::Phone, at(18, 7, 0), at(18, 7, 20));
    assert_eq!(morning, 20 * MIN);
    h.idle(Who::Phone, at(18, 7, 20));
    // The lease the phone took and did not use stays with it: at most one
    // lease. Everything else comes back to the PC.
    let lease_left = h.phone.holding_at(at(18, 7, 20)).lease_left_secs();
    assert!(lease_left > 0 && lease_left <= 10 * MIN);
    let (pc, d) = h.play(Who::Pc, at(18, 9, 0), at(18, 23, 0));
    assert_eq!(pc, 120 * MIN - morning - lease_left);
    assert_eq!(reason(&d), Some(LockReason::TimeUsedUp));
    // Story (b). The phone now leaves. Its away allowance went to the PC, so
    // away it has only the rest of its lease. That is the price of one
    // budget: it was spent at home.
    assert_eq!(h.phone.holding_at(at(18, 23, 0)).away_left_secs(), 0);
    h.phone_home = false;
    let (away, d) = h.play(Who::Phone, at(18, 23, 0), at(18, 23, 59));
    assert_eq!(away, lease_left);
    assert_eq!(reason(&d), Some(LockReason::NeedsMaster));
    assert_eq!(morning + pc + away, 120 * MIN);
    h.phone_home = true;
    h.idle(Who::Phone, at(18, 23, 59));
    assert_eq!(h.pool(today), (120, 0, 0));
}

#[test]
fn nothing_is_taken_from_a_phone_out_of_reach() {
    // Story (c). The phone leaves at 07:00 holding its share and is never
    // heard from. The PC asks for more, the master would like to hear from
    // the phone, but nothing can be taken from a phone that cannot hear.
    let mut h = home(rules(120), at(18, 7, 0));
    let today = h.master.today(at(18, 7, 0));
    h.phone_home = false;
    let (pc, d) = h.play(Who::Pc, at(18, 9, 0), at(18, 23, 0));
    assert_eq!(pc, 60 * MIN);
    assert_eq!(reason(&d), Some(LockReason::HeldByAnotherDevice));
    assert_eq!(h.master.wanted_reports(at(18, 23, 0)), vec!["phone".to_string()]);
    assert_eq!(h.master.device_view("pc", today).unwrap().waiting_secs, 10 * MIN);
    assert_eq!(h.master.device_view("phone", today).unwrap().reserved_secs, Some(60 * MIN));
    // The phone, away all day, can still spend all of its share.
    let (away, _) = h.play(Who::Phone, at(18, 15, 0), at(18, 23, 0));
    assert_eq!(away, 60 * MIN);
    assert_eq!(pc + away, 120 * MIN);
}

#[test]
fn a_lowered_grant_that_is_lost_takes_nothing_twice_and_leaks_nothing() {
    // Story (d). No hub: the stories below send each message by hand.
    let mut h = home(rules(120), at(18, 7, 0));
    h.hub_on = false;
    let today = h.master.today(at(18, 7, 0));
    let (pc, d) = h.play(Who::Pc, at(18, 9, 0), at(18, 12, 0));
    assert_eq!(pc, 60 * MIN);
    assert_eq!(reason(&d), Some(LockReason::HeldByAnotherDevice));
    assert_eq!(h.master.device_view("pc", today).unwrap().waiting_secs, 10 * MIN);
    // The phone reports. Its grant lowers its away allowance by the 10
    // minutes the PC waits for, and is lost on the way.
    let lost = h.master.handle_report(&h.phone.report(at(18, 12, 0), false), at(18, 12, 0)).unwrap();
    assert!(lost.acknowledge_now);
    assert_eq!(lost.away_cap_secs, 50 * MIN);
    // The master still holds back the old 60 minutes: the phone may never
    // hear of the lower figure. The PC gets nothing yet.
    assert_eq!(h.pool(today), (60, 60, 0));
    h.exchange(Who::Pc, at(18, 12, 1), true);
    assert_eq!(h.pc.holding_at(at(18, 12, 1)).in_hand_secs(), 0);
    // The phone reports again. It is offered the same level again, not a
    // second cut, takes it, and acknowledges it.
    let again = h.exchange(Who::Phone, at(18, 12, 2), false);
    assert_eq!(again.away_cap_secs, 50 * MIN);
    assert_eq!(h.pool(today), (60, 50, 10));
    // The lost grant turns up late, and changes nothing.
    assert_eq!(h.phone.take_grant(&lost, at(18, 12, 3)), GrantOutcome::Stale);
    // The PC gets the 10 minutes.
    let (more, _) = h.play(Who::Pc, at(18, 12, 3), at(18, 12, 30));
    assert_eq!(more, 10 * MIN);
    assert_eq!(h.pool(today), (70, 50, 0));
    // The PC stops asking, and the phone spends the rest of its share.
    h.idle(Who::Pc, at(18, 12, 30));
    let (away, _) = h.play(Who::Phone, at(18, 13, 0), at(18, 23, 0));
    assert_eq!(away, 50 * MIN);
    assert_eq!(pc + more + away, 120 * MIN);
}

#[test]
fn a_lowered_grant_lost_just_before_the_phone_leaves_leaks_nothing() {
    let mut h = home(rules(120), at(18, 7, 0));
    h.hub_on = false;
    let (pc, _) = h.play(Who::Pc, at(18, 9, 0), at(18, 12, 0));
    assert_eq!(pc, 60 * MIN);
    // The lowering grant is lost, and the phone leaves at once: it still
    // holds its whole share, and the master never gave any of it away.
    let _lost = h.master.handle_report(&h.phone.report(at(18, 12, 0), false), at(18, 12, 0)).unwrap();
    h.phone_home = false;
    let (away, _) = h.play(Who::Phone, at(18, 13, 0), at(18, 23, 0));
    assert_eq!(away, 60 * MIN);
    let (more, d) = h.play(Who::Pc, at(18, 13, 0), at(18, 23, 0));
    assert_eq!(more, 0);
    assert_eq!(reason(&d), Some(LockReason::HeldByAnotherDevice));
    assert_eq!(pc + away, 120 * MIN);
}

#[test]
fn without_the_hub_the_time_comes_at_the_phones_next_report() {
    let mut h = home(rules(120), at(18, 7, 0));
    h.hub_on = false;
    let today = h.master.today(at(18, 7, 0));
    // The PC uses the free hour and then waits, asking every minute.
    let (pc, _) = h.play(Who::Pc, at(18, 9, 0), at(18, 11, 30));
    assert_eq!(pc, 60 * MIN);
    // The phone's regular report lowers its allowance; it acknowledges at
    // once, as the grant asks; the PC's next retry gets the time.
    h.exchange(Who::Phone, at(18, 11, 30), false);
    assert_eq!(h.pool(today), (60, 50, 10));
    let (more, _) = h.play(Who::Pc, at(18, 11, 31), at(18, 12, 0));
    assert_eq!(more, 10 * MIN);
}

#[test]
fn a_need_no_longer_asked_for_lapses_and_takes_nothing() {
    let mut h = home(rules(120), at(18, 7, 0));
    h.hub_on = false;
    let today = h.master.today(at(18, 7, 0));
    let (pc, _) = h.play(Who::Pc, at(18, 9, 0), at(18, 10, 30));
    assert_eq!(pc, 60 * MIN);
    assert_eq!(h.master.device_view("pc", today).unwrap().waiting_secs, 10 * MIN);
    // The child walks away and the PC stops asking. Four minutes later the
    // phone reports: the need has lapsed, and nothing is taken from it.
    h.exchange(Who::Phone, at(18, 10, 34), false);
    assert_eq!(h.master.device_view("pc", today).unwrap().waiting_secs, 0);
    assert_eq!(h.phone.holding_at(at(18, 10, 34)).away_cap_secs, 60 * MIN);
}

#[test]
fn when_the_need_has_passed_the_phone_gets_its_share_back_as_far_as_time_is_free() {
    let mut h = home(rules(120), at(18, 7, 0));
    let today = h.master.today(at(18, 7, 0));
    let (pc, _) = h.play(Who::Pc, at(18, 9, 0), at(18, 10, 10));
    assert_eq!(pc, 70 * MIN);
    h.idle(Who::Pc, at(18, 10, 10));
    // The PC took 10 minutes of the phone's share and holds a little lease.
    let phone_cap = h.phone.holding_at(at(18, 10, 10)).away_cap_secs;
    assert!(phone_cap < 60 * MIN);
    // The guardian gives 30 minutes of extra time: the phone's allowance goes
    // back up to its share, and the rest is free.
    h.master.add_extra_secs(today, 30 * MIN);
    h.exchange(Who::Phone, at(18, 11, 0), false);
    assert_eq!(h.phone.holding_at(at(18, 11, 0)).away_cap_secs, 60 * MIN);
    let free = h.master.pool(today).free_secs.unwrap();
    assert_eq!(free, 30 * MIN - (60 * MIN - phone_cap));
}

#[test]
fn a_phone_away_all_day_spends_its_share_and_the_pc_the_rest() {
    let mut h = home(rules(120), at(18, 7, 0));
    let today = h.master.today(at(18, 7, 0));
    // The phone leaves for school holding half the day for use away.
    h.phone_home = false;
    assert_eq!(h.phone.holding_at(at(18, 8, 0)).away_left_secs(), 60 * MIN);
    let (away, d) = h.play(Who::Phone, at(18, 9, 0), at(18, 12, 0));
    assert_eq!(away, 60 * MIN);
    assert_eq!(reason(&d), Some(LockReason::NeedsMaster));
    // Meanwhile the PC at home gets the other half, a lease at a time, then
    // waits for the phone, which is not home.
    let (pc, d) = h.play(Who::Pc, at(18, 13, 0), at(18, 18, 0));
    assert_eq!(pc, 60 * MIN);
    assert_eq!(reason(&d), Some(LockReason::HeldByAnotherDevice));
    // The phone comes home and reports: the day is used up, exactly, and
    // there is nothing to give the PC.
    h.phone_home = true;
    h.exchange(Who::Phone, at(18, 18, 30), false);
    h.hub(at(18, 18, 30));
    assert_eq!(h.pool(today), (120, 0, 0));
    let d = h.phone.decide(at(18, 18, 30), Some("game"));
    assert_eq!(reason(&d), Some(LockReason::TimeUsedUp));
    // The PC, asking again as it does every minute while locked, now hears
    // that nothing is held elsewhere either.
    h.exchange(Who::Pc, at(18, 18, 31), true);
    let d = h.pc.decide(at(18, 18, 31), Some("game"));
    assert_eq!(reason(&d), Some(LockReason::TimeUsedUp));
}

#[test]
fn both_in_use_at_once_both_count_and_never_pass_the_budget() {
    let mut h = home(rules(90), at(18, 7, 0));
    let mut t = at(18, 16, 0);
    let mut total = 0;
    // Ten minute turns, the PC and the phone both in use every minute.
    while t < at(18, 20, 0) {
        total += h.play(Who::Pc, t, t + 600).0;
        total += h.play(Who::Phone, t, t + 600).0;
        t += 600;
    }
    assert_eq!(total, 90 * MIN);
    let pool = h.master.pool(h.master.today(t));
    assert_eq!(pool.used_secs + pool.reserved_secs.unwrap(), 90 * MIN);
}

#[test]
fn a_phone_away_at_midnight_and_the_master_agree_on_the_new_day() {
    let mut h = home(rules(120), at(18, 7, 0));
    // The phone is last heard from on Wednesday evening.
    h.exchange(Who::Phone, at(18, 20, 0), true);
    h.phone_home = false;
    let thursday = h.master.today(at(19, 1, 0));
    // On Thursday the master holds back the phone's share of Thursday,
    // without hearing from it: the same amount the phone works out alone.
    assert_eq!(h.master.device_view("phone", thursday).unwrap().reserved_secs, Some(60 * MIN));
    assert_eq!(h.phone.holding_at(at(19, 8, 0)).away_left_secs(), 60 * MIN);
    let (pc, _) = h.play(Who::Pc, at(19, 9, 0), at(19, 23, 0));
    assert_eq!(pc, 60 * MIN);
    let (phone, _) = h.play(Who::Phone, at(19, 9, 0), at(19, 23, 0));
    assert_eq!(phone, 60 * MIN);
}

#[test]
fn a_cut_while_the_phone_is_away_can_pass_the_new_budget_by_more_than_a_lease() {
    // The one case the shared budget cannot hold: the guardian cuts today's
    // budget while the phone is out of reach. The phone cannot hear of it and
    // spends the away share it already holds. The total stays within the old
    // budget, but passes the new one by far more than one lease.
    let mut h = home(rules(120), at(18, 7, 0));
    h.phone_home = false;
    let today = h.master.today(at(18, 7, 0));
    let (pc, _) = h.play(Who::Pc, at(18, 9, 0), at(18, 9, 50));
    assert_eq!(pc, 50 * MIN);
    let mut cut = h.master.rules();
    cut.version = 2;
    cut.budgets.set(Weekday::Wednesday, DailyBudget::Minutes(30));
    h.master.set_rules(cut).unwrap();
    // The PC gets the new rules with its next grant and is locked at once.
    let (more, d) = h.play(Who::Pc, at(18, 10, 0), at(18, 12, 0));
    assert_eq!(more, 10 * MIN, "the PC still spends the lease it already held");
    assert_eq!(reason(&d), Some(LockReason::TimeUsedUp));
    let (away, _) = h.play(Who::Phone, at(18, 12, 0), at(18, 15, 0));
    assert_eq!(away, 60 * MIN);
    let total = pc + more + away;
    let lease = 10 * MIN;
    assert!(total > 30 * MIN + lease, "{total}");
    assert!(total <= 120 * MIN);
    // When the phone comes home the master sees it, and gives nothing more.
    h.phone_home = true;
    h.exchange(Who::Phone, at(18, 15, 30), false);
    h.hub(at(18, 15, 30));
    let pool = h.master.pool(today);
    assert_eq!(pool.used_secs, total);
    assert_eq!(pool.free_secs, Some(0));
    assert_eq!(pool.over_secs, total - 30 * MIN);
}

#[test]
fn a_cut_while_every_device_is_in_reach_costs_at_most_the_leases_already_handed_out() {
    let mut h = home(rules(120), at(18, 7, 0));
    h.exchange(Who::Phone, at(18, 8, 0), true);
    let (pc, _) = h.play(Who::Pc, at(18, 9, 0), at(18, 9, 25));
    assert_eq!(pc, 25 * MIN);
    // When the cut comes, 25 minutes are used and each device holds a lease
    // of 10 minutes; the phone also holds its away share of 60.
    let mut cut = h.master.rules();
    cut.version = 2;
    cut.budgets.set(Weekday::Wednesday, DailyBudget::Minutes(30));
    h.master.set_rules(cut).unwrap();
    // The phone hears of the cut at once. Half of 30 would be 15 minutes, but
    // the pool is already overdrawn, so its away share drops to nothing.
    h.exchange(Who::Phone, at(18, 9, 26), false);
    assert_eq!(h.phone.holding_at(at(18, 9, 26)).away_cap_secs, 0);
    let (pc_more, _) = h.play(Who::Pc, at(18, 9, 30), at(18, 12, 0));
    h.phone_home = false;
    let (phone, _) = h.play(Who::Phone, at(18, 12, 0), at(18, 15, 0));
    assert_eq!(pc_more, 10 * MIN);
    assert_eq!(phone, 10 * MIN);
    // Over the new budget by the two leases that were out, and no more.
    assert_eq!(pc + pc_more + phone, 25 * MIN + 2 * 10 * MIN);
}

#[test]
fn two_phones_never_hold_more_than_the_whole_budget_between_them() {
    let mut r = rules(120);
    r.away_share_percent = 75;
    let now = at(18, 7, 0);
    let mut master = MasterBook::new("asha", r).unwrap();
    let first = master.add_device("phone1", DeviceKind::Portable, now).unwrap();
    let second = master.add_device("phone2", DeviceKind::Portable, now).unwrap();
    assert_eq!((first.away_percent, first.away_cap_secs), (75, 90 * MIN));
    assert_eq!((second.away_percent, second.away_cap_secs), (25, 30 * MIN));
    let pc = master.add_device("pc", DeviceKind::OnMaster, now).unwrap();
    assert_eq!(pc.lease_total_secs, 0);
    assert_eq!(master.pool(master.today(now)).free_secs, Some(0));
}

#[test]
fn with_two_phones_at_home_the_first_to_report_gives_back_and_only_once() {
    let mut r = rules(120);
    r.away_share_percent = 25;
    let now = at(18, 7, 0);
    let mut master = MasterBook::new("asha", r).unwrap();
    let day = master.today(now);
    let mut pc = DeviceState::from_pairing(&master.add_device("pc", DeviceKind::OnMaster, now).unwrap(), now);
    let mut one = DeviceState::from_pairing(&master.add_device("phone1", DeviceKind::Portable, now).unwrap(), now);
    let mut two = DeviceState::from_pairing(&master.add_device("phone2", DeviceKind::Portable, now).unwrap(), now);
    // Each phone holds 30 minutes; the PC uses the free 60 and then waits.
    let mut t = at(18, 9, 0);
    loop {
        let grant = master.handle_report(&pc.report(t, true), t).unwrap();
        pc.take_grant(&grant, t);
        let held = pc.holding_at(t).in_hand_secs();
        if held == 0 {
            break;
        }
        pc.record_use("game", t, t + held as i64);
        t += held as i64;
    }
    assert_eq!(master.device_view("pc", day).unwrap().used_secs, 60 * MIN);
    assert_eq!(master.device_view("pc", day).unwrap().waiting_secs, 10 * MIN);
    // Both phones report before either acknowledges. Only the first is
    // lowered: the second sees that the first will give back enough.
    let g1 = master.handle_report(&one.report(t, false), t).unwrap();
    let g2 = master.handle_report(&two.report(t, false), t).unwrap();
    assert_eq!((g1.away_cap_secs, g1.acknowledge_now), (20 * MIN, true));
    assert_eq!((g2.away_cap_secs, g2.acknowledge_now), (30 * MIN, false));
    assert_eq!(master.wanted_reports(t), vec!["phone1".to_string()]);
    one.take_grant(&g1, t);
    two.take_grant(&g2, t);
    master.handle_report(&one.report(t, false), t).unwrap();
    assert_eq!(master.pool(day).free_secs, Some(10 * MIN));
    assert_eq!(master.wanted_reports(t), vec!["pc".to_string()]);
}

#[test]
fn if_the_phone_lowered_first_has_left_the_phone_at_home_gives_back_three_minutes_later() {
    let mut r = rules(120);
    r.away_share_percent = 25;
    let now = at(18, 7, 0);
    let mut master = MasterBook::new("asha", r).unwrap();
    let day = master.today(now);
    let mut pc = DeviceState::from_pairing(&master.add_device("pc", DeviceKind::OnMaster, now).unwrap(), now);
    let mut one = DeviceState::from_pairing(&master.add_device("phone1", DeviceKind::Portable, now).unwrap(), now);
    let mut two = DeviceState::from_pairing(&master.add_device("phone2", DeviceKind::Portable, now).unwrap(), now);
    let mut ask = |master: &mut MasterBook, t: i64| {
        let grant = master.handle_report(&pc.report(t, true), t).unwrap();
        pc.take_grant(&grant, t);
        let held = pc.holding_at(t).in_hand_secs();
        pc.record_use("game", t, t + held as i64);
        held
    };
    // The PC uses the free hour, then waits for 10 minutes.
    let mut t = at(18, 9, 0);
    loop {
        let held = ask(&mut master, t);
        if held == 0 {
            break;
        }
        t += held as i64;
    }
    assert_eq!(master.device_view("pc", day).unwrap().waiting_secs, 10 * MIN);
    // Phone 1 reports and is lowered, then leaves before its grant arrives.
    let lost = master.handle_report(&one.report(t, false), t).unwrap();
    assert!(lost.acknowledge_now);
    assert_eq!(master.device_view("phone1", day).unwrap().giving_back_secs, 10 * MIN);
    // A minute later phone 2, at home, reports: the master still counts on
    // phone 1, so phone 2 keeps its share.
    ask(&mut master, t + 60);
    let g = master.handle_report(&two.report(t + 60, false), t + 60).unwrap();
    two.take_grant(&g, t + 60);
    assert_eq!(g.away_cap_secs, 30 * MIN);
    // Three minutes pass without word from phone 1, the PC asking every
    // minute. Now phone 2 is the one asked, and gives back the 10 minutes.
    for minute in 2..=4 {
        ask(&mut master, t + minute * 60);
    }
    assert_eq!(master.device_view("phone1", day).unwrap().giving_back_secs, 0);
    assert_eq!(master.wanted_reports(t + 240), vec!["phone1".to_string(), "phone2".to_string()]);
    let g = master.handle_report(&two.report(t + 240, false), t + 240).unwrap();
    assert_eq!((g.away_cap_secs, g.acknowledge_now), (20 * MIN, true));
    two.take_grant(&g, t + 240);
    master.handle_report(&two.report(t + 240, false), t + 240).unwrap();
    assert_eq!(ask(&mut master, t + 241), 10 * MIN);
    // Phone 1, away, still holds its 30 minutes: the master kept them back,
    // so the day can never go over.
    assert_eq!(one.holding_at(t + 300).away_left_secs(), 30 * MIN);
    assert_eq!(master.pool(day).free_secs, Some(0));
    let pool = master.pool(day);
    assert!(pool.used_secs + pool.reserved_secs.unwrap() <= 120 * MIN);
}

#[test]
fn rules_with_no_limit_held_by_a_phone_away_hold_back_the_whole_day() {
    let mut r = rules(120);
    r.budgets.set(Weekday::Wednesday, DailyBudget::NoLimit);
    let mut h = home(r, at(18, 7, 0));
    h.phone_home = false;
    let today = h.master.today(at(18, 7, 0));
    // The phone leaves believing today has no limit. Then the guardian sets
    // a limit. The phone could spend any amount, so the PC gets nothing.
    let mut limited = h.master.rules();
    limited.version = 2;
    limited.budgets.set(Weekday::Wednesday, DailyBudget::Minutes(60));
    h.master.set_rules(limited).unwrap();
    // The hub hands new rules to the PC at once (on a day with no limit a
    // device has no reason to ask for time, so rules must be pushed).
    h.exchange(Who::Pc, at(18, 8, 0), false);
    let (pc, d) = h.play(Who::Pc, at(18, 9, 0), at(18, 10, 0));
    assert_eq!(pc, 0);
    assert_eq!(reason(&d), Some(LockReason::TimeUsedUp));
    assert_eq!(h.master.pool(today).reserved_secs, None);
    h.idle(Who::Pc, at(18, 10, 0));
    // The phone used 40 minutes away and comes home: it reports, gets the
    // new rules and acknowledges them. The PC then gets the 20 minutes left.
    let (phone, _) = h.play(Who::Phone, at(18, 11, 0), at(18, 11, 40));
    assert_eq!(phone, 40 * MIN);
    h.phone_home = true;
    h.exchange(Who::Phone, at(18, 12, 0), false);
    h.exchange(Who::Phone, at(18, 12, 1), false);
    assert_eq!(h.master.pool(today).reserved_secs, Some(0));
    let (pc, _) = h.play(Who::Pc, at(18, 13, 0), at(18, 15, 0));
    assert_eq!(pc, 20 * MIN);
}

#[test]
fn extra_time_goes_into_the_pool() {
    let mut h = home(rules(60), at(18, 7, 0));
    h.phone_home = false;
    let today = h.master.today(at(18, 7, 0));
    let (pc, d) = h.play(Who::Pc, at(18, 9, 0), at(18, 12, 0));
    assert_eq!(pc, 30 * MIN);
    assert_eq!(reason(&d), Some(LockReason::HeldByAnotherDevice));
    h.master.add_extra_secs(today, 15 * MIN);
    let (more, _) = h.play(Who::Pc, at(18, 12, 0), at(18, 14, 0));
    assert_eq!(more, 15 * MIN);
}

#[test]
fn use_across_midnight_is_cut_in_two_and_counted_once() {
    let mut h = home(rules(120), at(18, 7, 0));
    h.exchange(Who::Phone, at(18, 23, 0), true);
    let slices = h.phone.record_use("game", at(18, 23, 50), at(19, 0, 20));
    assert_eq!(slices.len(), 2);
    assert_eq!((slices[0].secs(), slices[1].secs()), (10 * MIN, 20 * MIN));
    assert_eq!(slices[1].day, slices[0].day + 1);
    let report = h.phone.report(at(19, 0, 20), false);
    for _ in 0..3 {
        h.master.handle_report(&report, at(19, 0, 21)).unwrap();
    }
    let wednesday = h.master.today(at(18, 12, 0));
    assert_eq!(h.master.device_view("phone", wednesday).unwrap().used_secs, 10 * MIN);
    assert_eq!(h.master.device_view("phone", wednesday + 1).unwrap().used_secs, 20 * MIN);
    assert_eq!(h.master.usage(wednesday + 1).len(), 1);
}

#[test]
fn an_old_report_arriving_late_changes_nothing() {
    let mut h = home(rules(120), at(18, 7, 0));
    let today = h.master.today(at(18, 7, 0));
    h.exchange(Who::Phone, at(18, 9, 0), true);
    h.phone.record_use("game", at(18, 9, 0), at(18, 9, 5));
    let old: Report = h.phone.report(at(18, 9, 5), true);
    h.phone.record_use("game", at(18, 9, 5), at(18, 9, 9));
    h.exchange(Who::Phone, at(18, 9, 9), true);
    let before = h.master.pool(today);
    let grant = h.master.handle_report(&old, at(18, 9, 10)).unwrap();
    assert_eq!(h.master.pool(today), before);
    // Its grant gives nothing new (the lease stays topped up to 9 minutes
    // used plus one lease), and the phone takes it harmlessly.
    assert_eq!(grant.lease_total_secs, 19 * MIN);
    h.phone.take_grant(&grant, at(18, 9, 10));
    assert_eq!(h.phone.holding_at(at(18, 9, 10)).lease_left_secs(), 10 * MIN);
}

#[test]
fn every_public_message_survives_a_round_trip_as_json() {
    let mut h = home(rules(120), at(18, 7, 0));
    let (pc, _) = h.play(Who::Pc, at(18, 9, 0), at(18, 12, 0));
    assert_eq!(pc, 120 * MIN, "the phone at home gave its share to the PC");
    h.phone.record_use("game", at(18, 12, 0), at(18, 12, 5));
    let report = h.phone.report(at(18, 12, 5), true);
    let grant = h.master.handle_report(&report, at(18, 12, 5)).unwrap();
    let decision = h.phone.decide(at(18, 12, 6), Some("game"));
    let pool = h.master.pool(h.master.today(at(18, 12, 6)));
    macro_rules! round_trip {
        ($value:expr, $t:ty) => {{
            let text = serde_json::to_string(&$value).unwrap();
            let back: $t = serde_json::from_str(&text).unwrap();
            assert_eq!(back, $value);
        }};
    }
    round_trip!(report, Report);
    round_trip!(grant, Grant);
    round_trip!(decision, Decision);
    round_trip!(pool, limo_core::PoolView);
    round_trip!(h.master.rules(), RuleSet);
    round_trip!(h.master, MasterBook);
    round_trip!(h.phone, DeviceState);
}
