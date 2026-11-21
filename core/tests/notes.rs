//! A device's notes for the guardian, and the use on the child's other
//! devices, told as short stories: notes carried in reports until confirmed
//! and taken once through lost, repeated, late and odd reports; a report
//! from a newer device that holds a kind this version does not know; the
//! grant saying how much the other devices used and whether all of it was
//! on the master PC, and the device keeping it for its day; and messages,
//! books and states saved before and by this version.

use std::collections::BTreeMap;

use limo_core::{
    CivilDate, DailyBudget, DeviceKind, DeviceState, GuardedPage, Grant, GrantOutcome, MAX_NOTES_PER_REPORT,
    MasterBook, MasterError, OthersUsed, PhoneNote, PhonePermission, ProtectionLevel, Report, ReportOutcome,
    RuleSet, SentNote, WeekBudgets,
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
    r
}

/// A device that keeps its notes as the phone's engine does: numbered, in
/// every report until a grant confirms them.
struct Device {
    state: DeviceState,
    notes: Vec<SentNote>,
    last_note: u64,
}

impl Device {
    fn note(&mut self, note: PhoneNote) -> SentNote {
        self.last_note += 1;
        let sent = SentNote { id: SentNote::id_for(&self.state.device_id(), self.last_note), note };
        self.notes.push(sent.clone());
        sent
    }

    fn report(&mut self, now: i64, want_more: bool) -> Report {
        let mut report = self.state.report(now, want_more);
        report.notes = self.notes.clone();
        report
    }

    fn take(&mut self, grant: &Grant, now: i64) -> GrantOutcome {
        let outcome = self.state.take_grant(grant, now);
        if outcome != GrantOutcome::NotForThisDevice {
            self.notes.retain(|n| !grant.confirmed_notes.contains(&n.id));
        }
        outcome
    }
}

/// A master and the devices of one child.
struct Family {
    master: MasterBook,
    devices: BTreeMap<String, Device>,
}

impl Family {
    fn new(rules: RuleSet, now: i64, devices: &[(&str, DeviceKind)]) -> Family {
        let mut f = Family { master: MasterBook::new("asha", rules).unwrap(), devices: BTreeMap::new() };
        for (id, kind) in devices {
            f.pair(id, *kind, now);
        }
        f
    }

    fn pair(&mut self, id: &str, kind: DeviceKind, now: i64) -> Grant {
        let welcome = self.master.add_device(id, kind, now).unwrap();
        let state = DeviceState::from_pairing(&welcome, now);
        self.devices.insert(id.to_string(), Device { state, notes: Vec::new(), last_note: 0 });
        welcome
    }

    fn device(&mut self, id: &str) -> &mut Device {
        self.devices.get_mut(id).unwrap()
    }

    /// One report and its grant, both delivered.
    fn exchange(&mut self, id: &str, now: i64) -> ReportOutcome {
        let report = self.device(id).report(now, false);
        let outcome = self.master.handle_report(&report, now).unwrap();
        assert_eq!(self.device(id).take(&outcome.grant, now), GrantOutcome::Taken);
        outcome
    }

    /// Use on a device, reported with its grant delivered.
    fn use_and_report(&mut self, id: &str, app: &str, from: i64, to: i64) -> Grant {
        self.device(id).state.record_use(app, from, to);
        self.exchange(id, to).grant
    }
}

fn fixture<T: serde::de::DeserializeOwned>(text: &str) -> T {
    serde_json::from_str(text).unwrap()
}

// Notes.

#[test]
fn notes_ride_on_reports_until_confirmed_and_are_taken_once() {
    let mut f = Family::new(rules(120), at(17, 20, 0), &[("pc", DeviceKind::OnMaster), ("phone", DeviceKind::Portable)]);
    let setup = f.device("phone").note(PhoneNote::SetupFinished { at: at(17, 20, 5) });
    let off = f.device("phone").note(PhoneNote::PermissionOff { permission: PhonePermission::Battery, at: at(18, 7, 0) });
    assert_eq!((setup.id.as_str(), off.id.as_str()), ("phone/note/1", "phone/note/2"));

    // The first report is lost; the second carries both notes again, and its
    // grant is lost: the phone keeps them.
    let lost = f.device("phone").report(at(18, 8, 0), false);
    let second = f.device("phone").report(at(18, 8, 1), false);
    let outcome = f.master.handle_report(&second, at(18, 8, 1)).unwrap();
    assert_eq!(outcome.new_notes, vec![setup.clone(), off.clone()]);
    assert_eq!(outcome.grant.confirmed_notes, vec![setup.id.clone(), off.id.clone()]);
    // A third note; the next report carries all three, the wrong way round.
    let restart = f.device("phone").note(PhoneNote::Restarted { at: at(18, 9, 0) });
    let mut third = f.device("phone").report(at(18, 9, 1), false);
    third.notes.reverse();
    let outcome = f.master.handle_report(&third, at(18, 9, 1)).unwrap();
    assert_eq!(outcome.new_notes, vec![restart.clone()], "only the new one is new");
    assert_eq!(outcome.grant.confirmed_notes.len(), 3);
    // That grant arrives: the phone drops all three, and its next report
    // carries none.
    assert_eq!(f.device("phone").take(&outcome.grant, at(18, 9, 1)), GrantOutcome::Taken);
    assert!(f.device("phone").notes.is_empty());
    assert!(f.device("phone").report(at(18, 9, 2), false).notes.is_empty());
    // The lost report and the second turn up late, and the third again:
    // nothing new, and still confirmed so that a device can forget them.
    for late in [&lost, &second, &third] {
        let outcome = f.master.handle_report(late, at(18, 9, 3)).unwrap();
        assert!(outcome.new_notes.is_empty());
        assert_eq!(outcome.grant.confirmed_notes.len(), late.notes.len());
    }
    // A stale grant still confirms: the device forgets what it names.
    let guard = f.device("phone").note(PhoneNote::GuardOpened { until: at(18, 10, 5), at: at(18, 10, 0) });
    let report = f.device("phone").report(at(18, 10, 1), false);
    let fresh = f.master.handle_report(&report, at(18, 10, 1)).unwrap();
    assert_eq!(fresh.new_notes, vec![guard.clone()]);
    let newer = f.exchange("phone", at(18, 10, 2));
    assert!(newer.new_notes.is_empty(), "taken already");
    assert!(f.device("phone").notes.is_empty());

    // A note twice in one report counts once; one whose id no device makes,
    // or that names another device, is confirmed and not taken.
    let mut odd = f.device("phone").report(at(18, 11, 0), false);
    let usb = SentNote { id: "phone/note/5".into(), note: PhoneNote::UsbDebugging { on: true, at: at(18, 11, 0) } };
    let made_up = SentNote { id: "phone/note/five".into(), ..usb.clone() };
    let other = SentNote { id: "pc/note/6".into(), ..usb.clone() };
    odd.notes = vec![usb.clone(), usb.clone(), made_up, other];
    let outcome = f.master.handle_report(&odd, at(18, 11, 0)).unwrap();
    assert_eq!(outcome.new_notes, vec![usb]);
    assert_eq!(outcome.grant.confirmed_notes, vec!["phone/note/5", "phone/note/5", "phone/note/five", "pc/note/6"]);

    // The mark is saved with the book: loaded again, the book takes nothing
    // twice and the next note once.
    let book: MasterBook = fixture(&serde_json::to_string(&f.master).unwrap());
    assert_eq!(book, f.master);
    f.master = book;
    assert!(f.master.handle_report(&third, at(18, 11, 1)).unwrap().new_notes.is_empty());
    f.device("phone").last_note = 5;
    let clock = f.device("phone").note(PhoneNote::ClockMoved { by_secs: 3_600, followed: false, at: at(18, 12, 0) });
    assert_eq!(f.exchange("phone", at(18, 12, 1)).new_notes, vec![clock]);
    // The PC's notes have marks of their own.
    let pc = f.device("pc").note(PhoneNote::Restarted { at: at(18, 12, 2) });
    assert_eq!(pc.id, "pc/note/1");
    assert_eq!(f.exchange("pc", at(18, 12, 3)).new_notes, vec![pc]);
}

#[test]
fn a_report_holding_more_notes_than_a_device_keeps_gives_the_newest() {
    let mut f = Family::new(rules(120), at(17, 20, 0), &[("phone", DeviceKind::Portable)]);
    for i in 0..(MAX_NOTES_PER_REPORT as i64 + 50) {
        f.device("phone").note(PhoneNote::Restarted { at: at(18, 8, 0) + i });
    }
    let mut report = f.device("phone").report(at(18, 9, 0), false);
    report.notes.reverse();
    let outcome = f.master.handle_report(&report, at(18, 9, 0)).unwrap();
    assert_eq!(outcome.new_notes.len(), MAX_NOTES_PER_REPORT);
    assert!(outcome.new_notes.iter().all(|n| n.id != "phone/note/50"), "the oldest are left out");
    assert!(outcome.new_notes.iter().any(|n| n.id == "phone/note/51") && outcome.new_notes.iter().any(|n| n.id == "phone/note/250"));
    assert_eq!(outcome.grant.confirmed_notes.len(), MAX_NOTES_PER_REPORT + 50, "all confirmed");
}

#[test]
fn notes_from_a_removed_device_are_refused_with_its_report() {
    let mut f = Family::new(rules(120), at(17, 20, 0), &[("pc", DeviceKind::OnMaster), ("phone", DeviceKind::Portable)]);
    f.device("phone").note(PhoneNote::PermissionOff { permission: PhonePermission::Protection, at: at(18, 8, 0) });
    let late = f.device("phone").report(at(18, 8, 0), false);
    f.master.remove_device("phone", at(18, 8, 1)).unwrap();
    let before = serde_json::to_string(&f.master).unwrap();
    assert_eq!(f.master.handle_report(&late, at(18, 8, 2)).unwrap_err(), MasterError::DeviceRemoved { device_id: "phone".into() });
    assert_eq!(serde_json::to_string(&f.master).unwrap(), before);
}

#[test]
fn every_kind_reads_and_writes_in_the_words_the_hub_and_the_phone_use() {
    let wed = 20_775;
    let cases = [
        (PhoneNote::SetupFinished { at: 5 }, r#"{"note":"setup_finished","at":5}"#),
        (
            PhoneNote::PermissionOff { permission: PhonePermission::UsageAccess, at: 5 },
            r#"{"note":"permission_off","permission":"usage_access","at":5}"#,
        ),
        (
            PhoneNote::PermissionOn { permission: PhonePermission::Notifications, at: 5 },
            r#"{"note":"permission_on","permission":"notifications","at":5}"#,
        ),
        (PhoneNote::Protection { level: ProtectionLevel::Strict, at: 5 }, r#"{"note":"protection","level":"strict","at":5}"#),
        (PhoneNote::UsbDebugging { on: false, at: 5 }, r#"{"note":"usb_debugging","on":false,"at":5}"#),
        (
            PhoneNote::GuardedPageOpened { page: GuardedPage::DeviceAdmin, day: wed, at: 5 },
            r#"{"note":"guarded_page_opened","page":"device_admin","day":20775,"at":5}"#,
        ),
        (PhoneNote::GuardOpened { until: 305, at: 5 }, r#"{"note":"guard_opened","until":305,"at":5}"#),
        (PhoneNote::Restarted { at: 5 }, r#"{"note":"restarted","at":5}"#),
        (
            PhoneNote::ClockMoved { by_secs: -3_600, followed: true, at: 5 },
            r#"{"note":"clock_moved","by_secs":-3600,"followed":true,"at":5}"#,
        ),
    ];
    for (note, json) in cases {
        assert_eq!(serde_json::to_string(&note).unwrap(), json);
        assert_eq!(fixture::<PhoneNote>(json), note);
    }
    let sent = SentNote { id: "phone/note/1".into(), note: PhoneNote::Restarted { at: 5 } };
    assert_eq!(serde_json::to_string(&sent).unwrap(), r#"{"id":"phone/note/1","note":{"note":"restarted","at":5}}"#);
    // A field added by a newer device is passed over; a kind or a value that
    // is not known is an error for one note on its own.
    assert_eq!(fixture::<PhoneNote>(r#"{"note":"restarted","at":5,"why":"battery"}"#), PhoneNote::Restarted { at: 5 });
    for bad in [r#"{"note":"permission_off","permission":"camera","at":9}"#, r#"{"note":"time_zone_changed","at":9}"#] {
        assert!(serde_json::from_str::<PhoneNote>(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_report_from_a_newer_device_with_a_kind_this_version_does_not_know_is_still_taken() {
    let mut f = Family::new(rules(120), at(17, 20, 0), &[("phone", DeviceKind::Portable)]);
    f.device("phone").state.record_use("org.example.game", at(18, 9, 0), at(18, 9, 10));
    let report = f.device("phone").report(at(18, 9, 10), false);
    let mut json: serde_json::Value = serde_json::to_value(&report).unwrap();
    json["notes"] = serde_json::json!([
        { "id": "phone/note/1", "note": { "note": "restarted", "at": 5 } },
        { "id": "phone/note/2", "note": { "note": "battery_low", "percent": 4, "at": 6 } },
        { "id": "phone/note/3", "note": { "note": "permission_off", "permission": "camera", "at": 7 } },
        "not a note",
        { "id": "phone/note/4", "note": { "note": "usb_debugging", "on": true, "at": 8, "seen_by": "watch" } },
    ]);
    let read: Report = serde_json::from_value(json).unwrap();
    assert_eq!(read.notes, vec![
        SentNote { id: "phone/note/1".into(), note: PhoneNote::Restarted { at: 5 } },
        SentNote { id: "phone/note/4".into(), note: PhoneNote::UsbDebugging { on: true, at: 8 } },
    ]);
    let outcome = f.master.handle_report(&read, at(18, 9, 10)).unwrap();
    assert_eq!((outcome.new_slices.len(), outcome.new_notes.len()), (1, 2), "the use and the known notes are taken");
    // A report with no notes is written as before, with no field for them.
    let plain = f.devices["phone"].state.clone().report(at(18, 9, 11), false);
    assert!(serde_json::to_value(&plain).unwrap().get("notes").is_none());
}

// The use on the child's other devices.

#[test]
fn the_grant_says_how_much_the_other_devices_used_and_whether_all_of_it_was_on_the_pc() {
    let t0 = at(17, 20, 0);
    let mut f = Family::new(rules(120), t0, &[("pc", DeviceKind::OnMaster), ("phone", DeviceKind::Portable)]);
    let wed = f.master.today(at(18, 8, 0));
    // Nothing used yet: the master says so.
    let g = f.exchange("phone", at(18, 8, 0)).grant;
    assert_eq!((g.others_used_secs, g.others_all_on_master), (Some(0), false));
    assert_eq!(f.devices["phone"].state.others_used(wed), Some(OthersUsed { day: wed, secs: 0, all_on_master: false }));

    // 17 minutes on the PC, then 31 on the phone.
    f.use_and_report("pc", "robloxplayerbeta.exe", at(18, 16, 0), at(18, 16, 17));
    let g = f.use_and_report("phone", "org.example.game", at(18, 17, 0), at(18, 17, 31));
    assert_eq!((g.others_used_secs, g.others_all_on_master), (Some(17 * MIN), true));
    assert_eq!(f.devices["phone"].state.others_used(wed), Some(OthersUsed { day: wed, secs: 17 * MIN, all_on_master: true }));
    // The PC hears of the phone's use, which is not on the master PC.
    let g = f.exchange("pc", at(18, 17, 32)).grant;
    assert_eq!((g.others_used_secs, g.others_all_on_master), (Some(31 * MIN), false));
    // With the phone away, the figure is the best known: it is not forgotten
    // with the time left.
    f.device("phone").state.lost_contact();
    assert_eq!(f.devices["phone"].state.others_used(wed).map(|o| o.secs), Some(17 * MIN));

    // A tablet's 5 minutes: the rest is no longer all on the PC.
    f.pair("tablet", DeviceKind::Portable, at(18, 18, 0));
    f.use_and_report("tablet", "org.example.video", at(18, 18, 0), at(18, 18, 5));
    let g = f.exchange("phone", at(18, 18, 6)).grant;
    assert_eq!((g.others_used_secs, g.others_all_on_master), (Some(22 * MIN), false));
    // Removed, the tablet's use still counts, as use on another device.
    f.master.remove_device("tablet", at(18, 18, 7)).unwrap();
    let g = f.exchange("phone", at(18, 18, 8)).grant;
    assert_eq!((g.others_used_secs, g.others_all_on_master), (Some(22 * MIN), false));

    // A stale grant changes nothing; one from a master that does not say
    // leaves the phone knowing nothing of the others for that day.
    let stale = g.clone();
    let mut said_nothing = f.exchange("phone", at(18, 18, 9)).grant;
    assert_eq!(f.device("phone").take(&stale, at(18, 18, 9)), GrantOutcome::Stale);
    assert_eq!(f.devices["phone"].state.others_used(wed).map(|o| o.secs), Some(22 * MIN));
    let mut older_master = f.devices["phone"].state.clone();
    said_nothing.number += 1;
    said_nothing.others_used_secs = None;
    said_nothing.others_all_on_master = false;
    assert_eq!(older_master.take_grant(&said_nothing, at(18, 18, 10)), GrantOutcome::Taken);
    assert_eq!(older_master.others_used(wed), None);

    // The next day, Wednesday's figure is not Thursday's.
    assert!(f.devices["phone"].state.others_used(wed).is_some());
    assert_eq!(f.devices["phone"].state.others_used(wed + 1), None);
    let g = f.exchange("phone", at(19, 8, 0)).grant;
    assert_eq!((g.day, g.others_used_secs), (wed + 1, Some(0)));
    assert_eq!(f.devices["phone"].state.others_used(wed + 1).map(|o| o.secs), Some(0));
}

// Saved before, and by, this version.

/// Messages and state saved before notes and the other devices' use: by the
/// code at "Add the rules core" (piece one), when books kept whole slices
/// (piece two), and with marks (piece three).
#[test]
fn reports_grants_books_and_states_saved_before_notes_still_read() {
    let report: Report = fixture(include_str!("fixtures/piece_one_report.json"));
    let grant: Grant = fixture(include_str!("fixtures/piece_one_grant.json"));
    assert!(report.notes.is_empty());
    assert!(grant.confirmed_notes.is_empty() && grant.others_used_secs.is_none() && !grant.others_all_on_master);
    let repeated: Report = fixture(include_str!("fixtures/piece_two_repeated_report.json"));
    assert!(repeated.notes.is_empty());
    for state in [
        include_str!("fixtures/piece_one_phone_state.json"),
        include_str!("fixtures/piece_two_phone_state.json"),
        include_str!("fixtures/piece_three_phone_state.json"),
    ] {
        let state: DeviceState = fixture(state);
        assert_eq!(state.others_used(20_775), None);
    }

    // A book saved with marks takes a phone's first notes from number 1, and
    // its grant says what the others used; the phone's old state keeps it.
    let mut book: MasterBook = fixture(include_str!("fixtures/piece_three_master_book.json"));
    let mut phone: DeviceState = fixture(include_str!("fixtures/piece_three_phone_state.json"));
    let (wed, now) = (20_775, 1_794_985_200 + 180);
    let mut report = phone.report(now, false);
    report.notes = vec![SentNote { id: "phone/note/1".into(), note: PhoneNote::SetupFinished { at: now - 60 } }];
    let outcome = book.handle_report(&report, now).unwrap();
    assert_eq!(outcome.new_notes, report.notes);
    let others: u64 = book.counted_use(wed).iter().filter(|c| c.device_id != "phone").map(|c| c.counted_secs).sum();
    assert!(others > 0);
    assert_eq!(outcome.grant.others_used_secs, Some(others));
    assert!(!outcome.grant.others_all_on_master, "the removed tablet used some of it");
    assert_eq!(phone.take_grant(&outcome.grant, now), GrantOutcome::Taken);
    assert_eq!(phone.others_used(wed), Some(OthersUsed { day: wed, secs: others, all_on_master: false }));
    // A book saved by piece one takes notes too.
    let mut old: MasterBook = fixture(include_str!("fixtures/piece_one_master_book.json"));
    let mut old_phone: DeviceState = fixture(include_str!("fixtures/piece_one_phone_state.json"));
    let mut report = old_phone.report(1_794_976_800 + 60, false);
    report.notes = vec![SentNote { id: "phone/note/1".into(), note: PhoneNote::Restarted { at: 1 } }];
    assert_eq!(old.handle_report(&report, 1_794_976_800 + 60).unwrap().new_notes.len(), 1);
}

/// Saved by the code at this change: a PC used for 17 minutes and a phone
/// for 31, the phone's report carrying one note of each kind, its grant,
/// and the book and the phone's state after it.
#[test]
fn a_book_state_report_and_grant_saved_by_this_version_load_and_go_on() {
    let text = include_str!("fixtures/piece_four_master_book.json");
    let mut book: MasterBook = fixture(text);
    let mut phone: DeviceState = fixture(include_str!("fixtures/piece_four_phone_state.json"));
    let report: Report = fixture(include_str!("fixtures/piece_four_report.json"));
    let grant: Grant = fixture(include_str!("fixtures/piece_four_grant.json"));
    let (wed, now) = (20_775, at(18, 17, 40));
    assert_eq!(fixture::<MasterBook>(&serde_json::to_string(&book).unwrap()), book);
    assert_eq!(fixture::<Report>(&serde_json::to_string(&report).unwrap()), report);
    assert_eq!(fixture::<Grant>(&serde_json::to_string(&grant).unwrap()), grant);
    assert_eq!(fixture::<DeviceState>(&serde_json::to_string(&phone).unwrap()), phone);

    assert_eq!(report.notes.len(), 9, "one of each kind");
    assert_eq!(grant.confirmed_notes, report.notes.iter().map(|n| n.id.clone()).collect::<Vec<_>>());
    assert_eq!((grant.others_used_secs, grant.others_all_on_master), (Some(17 * MIN), true));
    assert_eq!(phone.others_used(wed), Some(OthersUsed { day: wed, secs: 17 * MIN, all_on_master: true }));
    // The report again brings nothing new; the phone's next note is new.
    assert!(book.handle_report(&report, now).unwrap().new_notes.is_empty());
    let mut next = phone.report(now, false);
    let tenth = SentNote { id: "phone/note/10".into(), note: PhoneNote::PermissionOff { permission: PhonePermission::Phone, at: now } };
    next.notes = vec![tenth.clone()];
    let outcome = book.handle_report(&next, now).unwrap();
    assert_eq!(outcome.new_notes, vec![tenth]);
    assert_eq!(phone.take_grant(&outcome.grant, now), GrantOutcome::Taken);
    assert_eq!(phone.others_used(wed).map(|o| o.secs), Some(17 * MIN));
}
