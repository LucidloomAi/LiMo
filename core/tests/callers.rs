//! What the master's first real caller, the hub, needs from the book, told
//! as short stories: removing a device, what a report brought for the first
//! time, a book that stays small, new rules as news, the next rules version
//! set by the book, what the master knows of a device, forgetting days, and
//! books saved by earlier versions.

use std::collections::BTreeMap;

use limo_core::{
    CivilDate, CountedUse, DailyBudget, DeviceKind, DeviceState, ExtraSource, Grant,
    GrantOutcome, MasterBook, MasterError, PinAction, PinError, Report, ReportOutcome, RuleSet, Slice,
    TryOutcome, WeekBudgets, next_code,
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

/// A master and the devices of one child.
struct Family {
    master: MasterBook,
    devices: BTreeMap<String, DeviceState>,
    welcomes: BTreeMap<String, Grant>,
}

impl Family {
    fn new(rules: RuleSet, now: i64, devices: &[(&str, DeviceKind)]) -> Family {
        let mut f = Family { master: MasterBook::new("asha", rules).unwrap(), devices: BTreeMap::new(), welcomes: BTreeMap::new() };
        for (id, kind) in devices {
            f.pair(id, *kind, now);
        }
        f
    }

    fn pair(&mut self, id: &str, kind: DeviceKind, now: i64) -> Grant {
        let welcome = self.master.add_device(id, kind, now).unwrap();
        self.devices.insert(id.to_string(), DeviceState::from_pairing(&welcome, now));
        self.welcomes.insert(id.to_string(), welcome.clone());
        welcome
    }

    fn device(&mut self, id: &str) -> &mut DeviceState {
        self.devices.get_mut(id).unwrap()
    }

    /// One report and its grant, both delivered, and the report again at
    /// once when the grant asks for it. Returns the first outcome.
    fn exchange(&mut self, id: &str, now: i64, want_more: bool) -> ReportOutcome {
        let report = self.device(id).report(now, want_more);
        let first = self.master.handle_report(&report, now).unwrap();
        assert_eq!(self.device(id).take_grant(&first.grant, now), GrantOutcome::Taken);
        if first.grant.acknowledge_now {
            let report = self.device(id).report(now, want_more);
            let again = self.master.handle_report(&report, now).unwrap();
            self.device(id).take_grant(&again.grant, now);
        }
        first
    }

    /// Use recorded on a device, not yet reported.
    fn use_app(&mut self, id: &str, app: &str, from: i64, to: i64) -> Vec<Slice> {
        self.device(id).record_use(app, from, to)
    }
}

fn book_json(book: &MasterBook) -> String {
    serde_json::to_string(book).unwrap()
}

// Removing a device.

#[test]
fn a_removed_phone_keeps_its_use_frees_what_it_held_and_its_share_at_once() {
    // An away share of 75 percent: the first phone gets 75 (90 minutes of a
    // two hour day), the second the other 25 (30 minutes). The PC gets none.
    let mut r = rules(120);
    r.away_share_percent = 75;
    let t0 = at(17, 20, 0);
    let mut f = Family::new(r, t0, &[("pc", DeviceKind::OnMaster), ("phone1", DeviceKind::Portable), ("phone2", DeviceKind::Portable)]);
    assert_eq!((f.welcomes["phone1"].away_percent, f.welcomes["phone2"].away_percent), (75, 25));
    let wed = f.master.today(at(18, 8, 0));
    // Phone 1 is used for half an hour away from home, and reports at home.
    f.exchange("phone1", at(18, 8, 0), false);
    f.use_app("phone1", "video", at(18, 8, 30), at(18, 9, 0));
    f.exchange("phone1", at(18, 9, 0), false);
    f.exchange("phone2", at(18, 9, 0), false);
    let before = f.master.pool(wed);
    assert_eq!((before.used_secs, before.reserved_secs, before.free_secs), (30 * MIN, Some(90 * MIN), Some(0)));
    assert_eq!(f.master.time_left_for("pc", wed), Ok(Some(0)));

    // 10:00 Phone 1 is lost: the guardian removes it.
    f.master.remove_device("phone1", at(18, 10, 0)).unwrap();
    let after = f.master.pool(wed);
    // Its half hour still counts; its 60 minutes of away time are free at
    // once, before any device reports.
    assert_eq!((after.used_secs, after.reserved_secs, after.free_secs), (30 * MIN, Some(30 * MIN), Some(60 * MIN)));
    assert_eq!(f.master.time_left_for("pc", wed), Ok(Some(60 * MIN)));
    assert_eq!(f.master.device_ids(), vec!["pc".to_string(), "phone2".to_string()]);
    assert_eq!(f.master.removed_device_ids(), vec!["phone1".to_string()]);
    assert_eq!(f.master.counted_use(wed), vec![
        CountedUse { device_id: "pc".into(), counted_secs: 0, removed: false },
        CountedUse { device_id: "phone2".into(), counted_secs: 0, removed: false },
        CountedUse { device_id: "phone1".into(), counted_secs: 30 * MIN, removed: true },
    ]);
    // Its share no longer caps phone 2's: at its next report phone 2 gets
    // the whole 75 percent, for today what the pool can cover, and for
    // later days.
    let grant = f.exchange("phone2", at(18, 10, 1), false).grant;
    assert_eq!((grant.away_percent, grant.away_cap_secs), (75, 90 * MIN));
    assert_eq!(f.master.pool(wed).free_secs, Some(0));
    // The PC asks: phone 2, at home, gives back what the PC is waiting for.
    let lease = f.exchange("pc", at(18, 10, 2), true).grant;
    assert_eq!(lease.lease_total_secs, 0);
    assert_eq!(f.master.wanted_reports(at(18, 10, 2)), vec!["phone2".to_string()]);
    f.exchange("phone2", at(18, 10, 2), false);
    assert_eq!(f.exchange("pc", at(18, 10, 3), true).grant.lease_total_secs, 10 * MIN);
    // A new phone paired in its place gets no share while phone 2 may hold
    // 75 of the 100 percent... only the 25 left.
    assert_eq!(f.pair("phone3", DeviceKind::Portable, at(18, 11, 0)).away_percent, 25);
    // Tomorrow the removed phone holds nothing, and its use stays on
    // Wednesday only.
    assert!(f.master.counted_use(wed + 1).iter().all(|c| !c.removed && c.counted_secs == 0));
}

#[test]
fn anything_from_a_removed_device_is_refused_and_changes_nothing() {
    let t0 = at(17, 20, 0);
    let mut f = Family::new(rules(120), t0, &[("pc", DeviceKind::OnMaster), ("phone", DeviceKind::Portable)]);
    f.master.set_code_secret("phone", vec![7; 32]).unwrap();
    f.exchange("phone", at(18, 9, 0), true);
    f.use_app("phone", "game", at(18, 9, 0), at(18, 9, 10));
    let sent_late = f.device("phone").report(at(18, 9, 10), true);
    f.master.remove_device("phone", at(18, 9, 15)).unwrap();
    let before = book_json(&f.master);

    // The report it sent before it was removed turns up late.
    let err = f.master.handle_report(&sent_late, at(18, 9, 20)).unwrap_err();
    assert_eq!(err, MasterError::DeviceRemoved { device_id: "phone".into() });
    let message = err.to_string();
    assert!(message.ends_with('.') && !message.contains(" - ") && message.contains("removed"), "{message}");
    assert_eq!(book_json(&f.master), before, "the book did not change");
    // Removing it twice, or a device never paired, is an error; so is
    // anything else asked of it. Its id is never paired again.
    assert_eq!(f.master.remove_device("phone", at(18, 9, 21)), Err(MasterError::DeviceRemoved { device_id: "phone".into() }));
    assert_eq!(f.master.remove_device("tablet", at(18, 9, 21)), Err(MasterError::UnknownDevice { device_id: "tablet".into() }));
    let wed = f.master.today(at(18, 9, 0));
    let removed = Err(MasterError::DeviceRemoved { device_id: "phone".into() });
    assert_eq!(f.master.device_view("phone", wed).map(|_| ()), removed);
    assert_eq!(f.master.time_left_for("phone", wed).map(|_| ()), removed);
    assert_eq!(f.master.code_secret("phone").map(|_| ()), removed);
    assert_eq!(f.master.set_code_secret("phone", vec![7; 32]), removed);
    assert_eq!(f.master.add_device("phone", DeviceKind::Portable, at(18, 9, 22)).map(|_| ()), removed);
    assert_eq!(book_json(&f.master), before);
    assert!(!f.master.wanted_reports(at(18, 9, 22)).contains(&"phone".to_string()));
    // The PC goes on as before.
    assert!(f.master.handle_report(&f.devices["pc"].clone().report(at(18, 9, 23), false), at(18, 9, 23)).is_ok());
}

#[test]
fn time_added_on_a_removed_device_that_it_did_not_use_is_dropped() {
    let t0 = at(17, 20, 0);
    let mut f = Family::new(rules(120), t0, &[("pc", DeviceKind::OnMaster), ("phone", DeviceKind::Portable)]);
    let secret: Vec<u8> = (1u8..=32).collect();
    f.master.set_code_secret("phone", secret.clone()).unwrap();
    f.device("phone").set_code_secret(secret.clone()).unwrap();
    let wed = f.master.today(at(18, 9, 0));
    // A 30 minute code on the phone, which holds no lease: it spends the
    // code's time first, 10 minutes of it, then reports.
    let code = next_code(secret, "phone", wed, 30, vec![]).unwrap();
    assert!(matches!(f.device("phone").try_code(&code.shown, at(18, 9, 0)), TryOutcome::Added { .. }));
    f.use_app("phone", "game", at(18, 9, 0), at(18, 9, 10));
    let outcome = f.exchange("phone", at(18, 9, 10), false);
    assert_eq!(outcome.new_added.len(), 1);
    let before = f.master.pool(wed);
    // 30 extra; 10 used; held back: 60 of away time and the 20 code minutes left.
    assert_eq!((before.extra_secs, before.used_secs, before.reserved_secs, before.free_secs),
        (30 * MIN, 10 * MIN, Some(80 * MIN), Some(60 * MIN)));

    f.master.remove_device("phone", at(18, 9, 30)).unwrap();
    let after = f.master.pool(wed);
    // The 10 minutes it used stay counted, as extra it was given and used.
    // The 20 it did not use are dropped: they were the phone's, and nobody
    // else may spend them. Its 60 minutes of away time are free.
    assert_eq!((after.extra_secs, after.used_secs, after.reserved_secs, after.free_secs),
        (10 * MIN, 10 * MIN, Some(0), Some(120 * MIN)));
    let pieces = f.master.extra_time(wed);
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].source, ExtraSource::Code { device_id: "phone".into(), selector: 10 });
    assert_eq!((pieces[0].secs, pieces[0].dropped_secs), (30 * MIN, 20 * MIN));
}

#[test]
fn use_a_removed_phone_never_reported_is_not_counted() {
    // Where the guarantee stops: the phone at school spends 30 minutes of its
    // away allowance and is lost there. The master frees what it held back
    // for it, which included those 30 minutes.
    let t0 = at(17, 20, 0);
    let mut f = Family::new(rules(120), t0, &[("pc", DeviceKind::OnMaster), ("phone", DeviceKind::Portable)]);
    let wed = f.master.today(at(18, 9, 0));
    let unreported: u64 = f.use_app("phone", "game", at(18, 9, 0), at(18, 9, 30)).iter().map(Slice::secs).sum();
    assert_eq!(f.master.device_view("phone", wed).unwrap().reserved_secs, Some(60 * MIN));
    f.master.remove_device("phone", at(18, 12, 0)).unwrap();
    // The PC is given the whole day.
    assert_eq!(f.master.time_left_for("pc", wed), Ok(Some(120 * MIN)));
    // So the day can go over by what the phone used and never reported, at
    // most what was held back for it.
    assert_eq!(unreported, 30 * MIN);
}

// What a report brought for the first time.

#[test]
fn the_outcome_names_each_new_slice_piece_and_request_once() {
    let t0 = at(17, 20, 0);
    let mut f = Family::new(rules(120), t0, &[("pc", DeviceKind::OnMaster), ("phone", DeviceKind::Portable)]);
    let secret: Vec<u8> = (1u8..=32).collect();
    f.master.set_code_secret("phone", secret.clone()).unwrap();
    f.device("phone").set_code_secret(secret.clone()).unwrap();
    let wed = f.master.today(at(18, 9, 0));
    let request = f.device("phone").ask_for_time(20, "Homework", at(18, 9, 0)).unwrap();
    let code = next_code(secret, "phone", wed, 15, vec![]).unwrap();
    let TryOutcome::Added { added } = f.device("phone").try_code(&code.shown, at(18, 9, 1)) else { panic!() };
    let mut slices = f.use_app("phone", "game", at(18, 9, 2), at(18, 9, 5));
    slices.extend(f.use_app("phone", "clock", at(18, 9, 5), at(18, 9, 6)));
    let first: Report = f.device("phone").report(at(18, 9, 6), false);

    let outcome = f.master.handle_report(&first, at(18, 9, 6)).unwrap();
    assert!(outcome.newest);
    // Both slices are new, the one that counts and the clock that does not.
    assert_eq!(outcome.new_slices, slices);
    assert_eq!(outcome.new_added, vec![added.clone()]);
    assert_eq!(outcome.new_requests, vec![request.clone()]);
    assert_eq!(outcome.grant.confirmed_slices.len(), 2);
    // The same report again brings nothing new, and is not newer.
    let again = f.master.handle_report(&first, at(18, 9, 7)).unwrap();
    assert!(again.new_slices.is_empty() && again.new_added.is_empty() && again.new_requests.is_empty());
    assert!(!again.newest);
    assert_eq!(again.grant.confirmed_slices.len(), 2, "still confirmed, so the device can forget them");
    // The device never had the grant: its next report carries everything
    // again, with a new slice. Only the new slice is new.
    let later = f.use_app("phone", "game", at(18, 9, 10), at(18, 9, 12));
    let second = f.device("phone").report(at(18, 9, 12), false);
    assert_eq!(second.slices.len(), 3);
    let outcome = f.master.handle_report(&second, at(18, 9, 12)).unwrap();
    assert_eq!(outcome.new_slices, later);
    assert!(outcome.new_added.is_empty() && outcome.new_requests.is_empty() && outcome.newest);
    // The first report turns up once more, late: nothing.
    let late = f.master.handle_report(&first, at(18, 9, 13)).unwrap();
    assert!(late.new_slices.is_empty() && !late.newest);
    assert_eq!(f.master.device_view("phone", wed).unwrap().used_secs, 5 * MIN);
    assert_eq!(f.master.extra_time(wed).len(), 1);
    assert_eq!(f.master.requests(wed).len(), 1);
}

#[test]
fn slices_lost_repeated_late_and_out_of_order_count_once() {
    let t0 = at(17, 20, 0);
    let mut f = Family::new(rules(600), t0, &[("phone", DeviceKind::Portable)]);
    let wed = f.master.today(at(18, 9, 0));
    // Slices 1 to 3 on Wednesday; the report that carries them is lost.
    for m in [0, 10, 20] {
        f.use_app("phone", "game", at(18, 22, m), at(18, 22, m + 5));
    }
    let lost = f.device("phone").report(at(18, 22, 30), false);
    // Slice 4; the next report arrives with its slices the wrong way round,
    // and its grant is lost: the phone keeps them all.
    f.use_app("phone", "game", at(18, 22, 30), at(18, 22, 35));
    let mut backwards = f.device("phone").report(at(18, 22, 40), false);
    backwards.slices.reverse();
    let outcome = f.master.handle_report(&backwards, at(18, 22, 40)).unwrap();
    assert_eq!(outcome.new_slices.len(), 4);
    assert_eq!(f.master.device_view("phone", wed).unwrap().used_secs, 20 * MIN);
    // Slices 5 and 6: use across midnight, cut in two.
    let across = f.use_app("phone", "game", at(18, 23, 50), at(19, 0, 20));
    assert_eq!(across.len(), 2);
    let report = f.device("phone").report(at(19, 0, 20), false);
    assert_eq!(report.slices.len(), 6);
    let outcome = f.master.handle_report(&report, at(19, 0, 21)).unwrap();
    assert_eq!(outcome.new_slices, across);
    // The lost report turns up after all, and the newest one comes again.
    assert!(f.master.handle_report(&lost, at(19, 0, 22)).unwrap().new_slices.is_empty());
    assert!(f.master.handle_report(&report, at(19, 0, 23)).unwrap().new_slices.is_empty());
    assert_eq!(f.master.device_view("phone", wed).unwrap().used_secs, 30 * MIN);
    assert_eq!(f.master.device_view("phone", wed + 1).unwrap().used_secs, 20 * MIN);
    // A slice twice in one report counts once; one whose id no device makes
    // is confirmed and not counted.
    let mut odd = f.device("phone").report(at(19, 0, 30), false);
    let mut twice = f.use_app("phone", "game", at(19, 0, 30), at(19, 0, 40));
    twice.push(twice[0].clone());
    let mut made_up = twice[0].clone();
    made_up.id = "phone/seven".into();
    odd.slices = twice.clone();
    odd.slices.push(made_up);
    let outcome = f.master.handle_report(&odd, at(19, 0, 40)).unwrap();
    assert_eq!(outcome.new_slices, vec![twice[0].clone()]);
    assert!(outcome.grant.confirmed_slices.contains(&"phone/seven".to_string()));
    assert_eq!(f.master.device_view("phone", wed + 1).unwrap().used_secs, 30 * MIN);
}

#[test]
fn a_device_that_lost_its_state_is_not_counted_on_a_day_it_already_reported() {
    // Where the guarantee stops: the phone is restored from a backup made
    // after its first slice, and numbers its slices from 2 again.
    let t0 = at(17, 20, 0);
    let mut f = Family::new(rules(600), t0, &[("phone", DeviceKind::Portable)]);
    let wed = f.master.today(at(18, 9, 0));
    f.use_app("phone", "game", at(18, 9, 0), at(18, 9, 10));
    let backup = serde_json::to_string(&f.devices["phone"]).unwrap();
    f.use_app("phone", "game", at(18, 10, 0), at(18, 10, 10));
    f.use_app("phone", "game", at(18, 11, 0), at(18, 11, 10));
    f.exchange("phone", at(18, 11, 10), false);
    assert_eq!(f.master.device_view("phone", wed).unwrap().used_secs, 30 * MIN);
    // Restored: the slice from 09:00 is sent again, and new use made today
    // is numbered 2 and 3, at or below the mark of 3.
    *f.device("phone") = serde_json::from_str(&backup).unwrap();
    f.use_app("phone", "game", at(18, 12, 0), at(18, 12, 10));
    f.use_app("phone", "game", at(18, 13, 0), at(18, 13, 10));
    let outcome = f.master.handle_report(&f.devices["phone"].clone().report(at(18, 13, 10), false), at(18, 13, 10)).unwrap();
    assert!(outcome.new_slices.is_empty(), "taken as counted already");
    assert_eq!(f.master.device_view("phone", wed).unwrap().used_secs, 30 * MIN, "20 minutes are missing");
    // On a later day, with no mark yet, its slices count again.
    let next = f.use_app("phone", "game", at(19, 9, 0), at(19, 9, 10));
    let outcome = f.master.handle_report(&f.devices["phone"].clone().report(at(19, 9, 10), false), at(19, 9, 10)).unwrap();
    assert_eq!(outcome.new_slices, next);
    // The way out is to remove it and pair it again, under a new id.
    f.master.remove_device("phone", at(19, 9, 11)).unwrap();
    let again = f.pair("phone-2", DeviceKind::Portable, at(19, 9, 11));
    assert_eq!(again.device_id, "phone-2");
}

#[test]
fn a_busy_book_stays_small() {
    // Two devices, a slice every 108 seconds each and a report every two
    // minutes, for two days: 3,200 slices. A book that kept every slice was
    // 744,818 bytes of JSON (measured with the code before marks).
    let mut r = RuleSet::new(1, INDIA);
    r.budgets = WeekBudgets::every_day(DailyBudget::NoLimit);
    let mut book = MasterBook::new("child-0123456789abcdef", r).unwrap();
    let ids = ["device-1111111111111111", "device-2222222222222222"];
    let t0 = at(17, 0, 0);
    let mut devices = [
        DeviceState::from_pairing(&book.add_device(ids[0], DeviceKind::OnMaster, t0).unwrap(), t0),
        DeviceState::from_pairing(&book.add_device(ids[1], DeviceKind::Portable, t0).unwrap(), t0),
    ];
    let (mut slices, mut taken) = (0, 0);
    let mut t = t0;
    let mut next_report = t0;
    while t < t0 + 2 * 86_400 {
        for d in devices.iter_mut() {
            d.record_use(if slices % 3 == 0 { "com.example.game" } else { "com.example.video" }, t, t + 60);
            slices += 1;
        }
        t += 108;
        if t >= next_report {
            for d in devices.iter_mut() {
                let outcome = book.handle_report(&d.report(t, true), t).unwrap();
                taken += outcome.new_slices.len();
                d.take_grant(&outcome.grant, t);
            }
            next_report = t + 120;
        }
    }
    for d in devices.iter_mut() {
        taken += book.handle_report(&d.report(t, false), t).unwrap().new_slices.len();
    }
    assert_eq!((slices, taken), (3_200, 3_200));
    let bytes = book_json(&book).len();
    println!("A busy book of two days: {bytes} bytes of JSON");
    assert!(bytes < 6_000, "{bytes} bytes");
}

// New rules, the next version, and what the master knows of a device.

#[test]
fn new_rules_are_news_until_each_device_reports_holding_them() {
    let t0 = at(17, 20, 0);
    let mut f = Family::new(rules(120), t0, &[("pc", DeviceKind::OnMaster), ("phone", DeviceKind::Portable)]);
    f.exchange("pc", at(18, 8, 0), false);
    f.exchange("phone", at(18, 8, 0), false);
    assert!(f.master.wanted_reports(at(18, 8, 1)).is_empty());
    let wed = f.master.today(at(18, 8, 0));

    let mut shorter = f.master.rules();
    shorter.version = 99;
    shorter.lease_minutes = 5;
    let taken = f.master.change_rules(shorter).unwrap();
    assert_eq!((taken.version, taken.lease_minutes), (2, 5));
    assert_eq!(f.master.rules(), taken);
    // Every device is wanted, in reach or not: the book cannot know.
    assert_eq!(f.master.wanted_reports(at(18, 9, 0)), vec!["pc".to_string(), "phone".to_string()]);
    let view = f.master.device_view("pc", wed).unwrap();
    assert_eq!((view.rules_version, view.has_latest_news), (1, false));
    // The PC takes the new rules with its grant, and says so in its next
    // report.
    f.exchange("pc", at(18, 9, 1), false);
    assert_eq!(f.devices["pc"].rules_version(), 2);
    assert!(f.master.wanted_reports(at(18, 9, 1)).contains(&"pc".to_string()));
    f.exchange("pc", at(18, 9, 2), false);
    assert_eq!(f.master.wanted_reports(at(18, 9, 2)), vec!["phone".to_string()]);
    let view = f.master.device_view("pc", wed).unwrap();
    assert_eq!((view.rules_version, view.has_latest_news), (2, true));
    // A device paired now starts with the current rules and news.
    f.pair("tablet", DeviceKind::Portable, at(18, 9, 3));
    let view = f.master.device_view("tablet", wed).unwrap();
    assert_eq!((view.rules_version, view.has_latest_news), (2, true));
    assert_eq!(f.master.wanted_reports(at(18, 9, 3)), vec!["phone".to_string()]);
}

#[test]
fn the_book_sets_the_next_rules_version_itself() {
    let mut book = MasterBook::new("asha", rules(120)).unwrap();
    let mut next = rules(90);
    next.version = 0;
    assert_eq!(book.change_rules(next.clone()).unwrap().version, 2);
    next.version = 1;
    assert_eq!(book.change_rules(next.clone()).unwrap().version, 3);
    let mut bad = next.clone();
    bad.lease_minutes = 0;
    assert!(matches!(book.change_rules(bad), Err(MasterError::InvalidRules { .. })));
    assert_eq!(book.rules().version, 3);
    // set_rules still takes a version of the caller's, above the current one.
    next.version = u32::MAX;
    book.set_rules(next.clone()).unwrap();
    assert_eq!(
        book.change_rules(next),
        Err(MasterError::RulesNotNewer { current: u32::MAX, given: u32::MAX })
    );
}

#[test]
fn the_master_knows_how_far_each_device_clock_is_from_its_own() {
    let t0 = at(17, 20, 0);
    let mut f = Family::new(rules(120), t0, &[("pc", DeviceKind::OnMaster), ("phone", DeviceKind::Portable)]);
    let wed = f.master.today(at(18, 9, 0));
    assert_eq!(f.master.device_view("phone", wed).unwrap().clock_off_by_secs, None);
    // The phone's clock is ten minutes behind the master's.
    let report = f.device("phone").report(at(18, 8, 50), false);
    f.master.handle_report(&report, at(18, 9, 0)).unwrap();
    assert_eq!(f.master.device_view("phone", wed).unwrap().clock_off_by_secs, Some(-600));
    // Put right: a newer report says so.
    let newer = f.device("phone").report(at(18, 9, 30), false);
    f.master.handle_report(&newer, at(18, 9, 30)).unwrap();
    assert_eq!(f.master.device_view("phone", wed).unwrap().clock_off_by_secs, Some(0));
    // An older report arriving late does not change what the newest said.
    f.master.handle_report(&report, at(18, 9, 40)).unwrap();
    assert_eq!(f.master.device_view("phone", wed).unwrap().clock_off_by_secs, Some(0));
    // The PC, a minute and a half ahead.
    let report = f.device("pc").report(at(18, 9, 31) + 30, false);
    f.master.handle_report(&report, at(18, 9, 30)).unwrap();
    assert_eq!(f.master.device_view("pc", wed).unwrap().clock_off_by_secs, Some(90));
}

#[test]
fn forgetting_days_says_whether_the_book_changed() {
    let t0 = at(17, 20, 0);
    let mut f = Family::new(rules(120), t0, &[("pc", DeviceKind::OnMaster)]);
    let wed = f.master.today(at(18, 9, 0));
    f.use_app("pc", "game", at(17, 21, 0), at(17, 21, 5));
    f.use_app("pc", "game", at(18, 9, 0), at(18, 9, 5));
    f.exchange("pc", at(18, 9, 5), false);
    assert!(f.master.forget_days_before(wed), "Tuesday was dropped");
    assert!(!f.master.forget_days_before(wed), "nothing more to drop");
    assert!(!f.master.forget_days_before(wed - 3), "an earlier day changes nothing");
    assert!(f.master.counted_use(wed - 1).iter().all(|c| c.counted_secs == 0));
    assert!(f.master.forget_days_before(wed + 1), "Wednesday was dropped");
    assert!(f.master.forget_days_before(wed + 2), "the mark moves on even with nothing to drop");
    assert!(!f.master.forget_days_before(wed + 2));
}

// Books saved by earlier versions.

fn fixture<T: serde::de::DeserializeOwned>(text: &str) -> T {
    serde_json::from_str(text).unwrap()
}

/// The book, the device states and a report saved by the code at the commit
/// "Add the hub store", when a book kept every slice: a PC, a phone and a
/// tablet over Tuesday and Wednesday, with extra time given, a request
/// granted, a code on the phone, the PIN on the tablet, a lock from the
/// master, and a four digit PIN, as that code allowed. The numbers below
/// are what that code showed for it.
#[test]
fn a_book_saved_with_whole_slices_loads_and_counts_the_same() {
    let text = include_str!("fixtures/piece_two_master_book.json");
    assert!(text.contains("\"slices\""));
    let mut book: MasterBook = fixture(text);
    let mut phone: DeviceState = fixture(include_str!("fixtures/piece_two_phone_state.json"));
    let mut tablet: DeviceState = fixture(include_str!("fixtures/piece_two_tablet_state.json"));
    let mut pc: DeviceState = fixture(include_str!("fixtures/piece_two_pc_state.json"));
    let repeated: Report = fixture(include_str!("fixtures/piece_two_repeated_report.json"));
    let (tue, wed, now) = (20_774, 20_775, 1_794_985_200);

    let pool = |b: &MasterBook, day| {
        let p = b.pool(day);
        (p.budget_secs, p.extra_secs, p.used_secs, p.reserved_secs, p.free_secs, p.over_secs)
    };
    assert_eq!(pool(&book, tue), (Some(7_200), 0, 2_700, Some(0), Some(4_500), 0));
    assert_eq!(pool(&book, wed), (Some(7_200), 4_200, 3_600, Some(7_800), Some(0), 0));
    let used = |b: &MasterBook, day| -> Vec<u64> { b.counted_use(day).iter().map(|c| c.counted_secs).collect() };
    assert_eq!(used(&book, tue), vec![1_200, 900, 600]);
    assert_eq!(used(&book, wed), vec![1_500, 1_800, 300]);
    let view = book.device_view("phone", wed).unwrap();
    assert_eq!((view.reserved_secs, view.added_secs), (Some(3_600), 1_800));
    let view = book.device_view("tablet", wed).unwrap();
    assert_eq!((view.reserved_secs, view.added_secs), (Some(4_200), 900));
    assert_eq!(book.time_left_for("phone", wed), Ok(Some(3_600)));
    assert_eq!(book.extra_time(wed).len(), 4);
    assert_eq!(book.wanted_reports(now), vec!["pc".to_string(), "phone".to_string(), "tablet".to_string()]);
    // Saved again, it has no slices: they were folded into marks.
    let saved = book_json(&book);
    assert!(!saved.contains("\"slices\"") && saved.contains("\"marks\""));
    assert!(saved.len() < text.len() / 2, "{} bytes, from {}", saved.len(), text.len());

    // Every slice the old book held, sent again, is not counted again.
    let old: serde_json::Value = serde_json::from_str(text).unwrap();
    for day in old["days"].as_array().unwrap() {
        let slices: Vec<Slice> = day["slices"].as_object().unwrap().values().map(|s| serde_json::from_value(s.clone()).unwrap()).collect();
        for device in ["pc", "phone", "tablet"] {
            let mine: Vec<Slice> = slices.iter().filter(|s| s.device_id == device).cloned().collect();
            if mine.is_empty() {
                continue;
            }
            let mut report = repeated.clone();
            report.device_id = device.into();
            report.message_number = 1;
            report.slices = mine;
            assert!(book.handle_report(&report, now).unwrap().new_slices.is_empty(), "{device}");
        }
    }
    assert!(book.handle_report(&repeated, now).unwrap().new_slices.is_empty());
    assert_eq!(pool(&book, tue).2, 2_700);
    assert_eq!(pool(&book, wed).2, 3_600);
    // The phone's slice not yet reported counts once.
    let unsent = phone.unconfirmed_slices();
    assert_eq!(unsent.len(), 1);
    let outcome = book.handle_report(&phone.report(now, false), now).unwrap();
    assert_eq!(outcome.new_slices, unsent);
    phone.take_grant(&outcome.grant, now);
    assert_eq!(pool(&book, wed).2, 3_600 + 180);
    // The PC, with nothing unsent, reports and takes its grant.
    let outcome = book.handle_report(&pc.report(now, false), now).unwrap();
    assert!(outcome.new_slices.is_empty() && outcome.newest);
    assert_eq!(pc.take_grant(&outcome.grant, now), GrantOutcome::Taken);

    // The phone holds the four digit PIN of that time: it still works there.
    assert_eq!(phone.try_pin("2468", PinAction::ConfirmRelease, now), Ok(TryOutcome::ReleaseConfirmed));
    assert!(matches!(phone.try_pin("1357", PinAction::ConfirmRelease, now), Ok(TryOutcome::Wrong { .. })));
    // No new four digit PIN can be set.
    assert_eq!(
        book.set_pin("2468", (40u8..56).collect()),
        Err(MasterError::BadPin { problem: PinError::NotSixToEightDigits })
    );

    // A device of the old book can be removed: the tablet, which added 15
    // minutes with the PIN on Wednesday and used 5 of them.
    let tablet_report = tablet.report(now, false);
    book.handle_report(&tablet_report, now).unwrap();
    book.remove_device("tablet", now + 60).unwrap();
    assert_eq!(used(&book, wed), vec![1_500, 1_800 + 180, 300]);
    let pin_piece = book.extra_time(wed).into_iter().find(|e| matches!(e.source, ExtraSource::Pin { .. })).unwrap();
    assert_eq!((pin_piece.secs, pin_piece.dropped_secs), (900, 600));
    assert_eq!(book.pool(wed).extra_secs, 4_200 - 600);
}

/// A book in the shape of this version (marks, removed devices, dropped
/// added time, clock offsets), saved by the code at this change: the piece
/// two book above after the phone reported, the tablet was removed and a
/// new tablet paired, with the phone's state at that moment.
#[test]
fn a_book_saved_with_marks_loads_and_goes_on_working() {
    let text = include_str!("fixtures/piece_three_master_book.json");
    let mut book: MasterBook = fixture(text);
    let mut phone: DeviceState = fixture(include_str!("fixtures/piece_three_phone_state.json"));
    let (wed, now) = (20_775, 1_794_985_200 + 120);
    assert!(text.contains("\"marks\"") && !text.contains("\"slices\""));
    assert_eq!(fixture::<MasterBook>(&book_json(&book)), book);
    assert_eq!(book.device_ids(), vec!["pc".to_string(), "phone".to_string(), "tablet-2".to_string()]);
    assert_eq!(book.removed_device_ids(), vec!["tablet".to_string()]);
    let p = book.pool(wed);
    assert_eq!((p.extra_secs, p.used_secs), (3_600, 3_780));
    assert_eq!(book.counted_use(wed).last(), Some(&CountedUse { device_id: "tablet".into(), counted_secs: 300, removed: true }));
    assert_eq!(book.device_view("phone", wed).unwrap().clock_off_by_secs, Some(0));
    // Both sides go on: what the phone sent before counts nothing more, its
    // new use counts once.
    let fresh = phone.record_use("video", now, now + 120);
    let outcome = book.handle_report(&phone.report(now + 120, false), now + 120).unwrap();
    assert_eq!(outcome.new_slices, fresh);
    assert_eq!(phone.take_grant(&outcome.grant, now + 120), GrantOutcome::Taken);
    assert_eq!(book.pool(wed).used_secs, 3_780 + 120);
    // Reports from the removed tablet are refused.
    let mut late = phone.report(now + 121, false);
    late.device_id = "tablet".into();
    late.slices.clear();
    assert_eq!(book.handle_report(&late, now + 121).unwrap_err(), MasterError::DeviceRemoved { device_id: "tablet".into() });
}
