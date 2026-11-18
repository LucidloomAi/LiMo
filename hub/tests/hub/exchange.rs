//! The exchange: saved before sent, all or nothing, and reports that do not
//! fit.

use std::collections::BTreeSet;

use limo_core::{DeviceKind, Report, Slice};
use limo_hub::{EventKind, Hub, HubError};

use crate::common::*;

const M: u64 = 60;

#[test]
fn every_grant_is_on_the_disk_before_it_is_returned() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 0));

    // A morning of use on both devices. After every exchange the hub is
    // closed and opened again: what it shows must be what it showed before,
    // and the next grant must carry on from the last one.
    let mut last_number = 0;
    let mut t = at(8, 0);
    for step in 0..12 {
        let device = if step % 3 == 0 { &mut phone } else { &mut pc };
        let app = if step % 2 == 0 { "game" } else { "chat" };
        device.state.record_use(app, t, t + 4 * MIN);
        t += 4 * MIN;
        let grant = exchange(&mut hub, device, t, true);
        assert!(grant.number > last_number, "grant numbers carry on after a reopen");
        last_number = grant.number;

        let before = (hub.child_today(&asha, t).unwrap(), hub.usage(&asha, day_of(t), day_of(t)).unwrap());
        hub.close().unwrap();
        hub = Hub::open(&place.path).unwrap();
        let after = (hub.child_today(&asha, t).unwrap(), hub.usage(&asha, day_of(t), day_of(t)).unwrap());
        assert_eq!(after, before, "step {step}");
    }
    let today = hub.child_today(&asha, t).unwrap();
    assert_eq!(today.used_secs, 48 * M);
    assert_eq!(hub.usage(&asha, day_of(t), day_of(t)).unwrap().secs, 48 * M);
}

#[test]
fn an_exchange_that_fails_part_way_leaves_nothing_behind() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    exchange(&mut hub, &mut pc, at(8, 0), true);
    let before = hub.child_today(&asha, at(8, 5)).unwrap();
    let events_before = hub.events(100, None).unwrap();
    hub.close().unwrap();

    // Make the step after the book is saved fail: keeping the slices.
    raw(&place.path)
        .execute_batch("CREATE TRIGGER fail BEFORE INSERT ON slice BEGIN SELECT RAISE(ABORT, 'test failure'); END;")
        .unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    pc.state.record_use("game", at(8, 0), at(8, 5));
    let report = pc.state.report(at(8, 5), true);
    let err = hub.handle_report(&report, at(8, 5)).unwrap_err();
    assert!(matches!(err, HubError::Storage { .. }), "{err:?}");
    // No grant left the hub, and nothing changed: not the book, not the
    // device's figures, not the events.
    assert_eq!(hub.child_today(&asha, at(8, 5)).unwrap(), before);
    assert_eq!(hub.events(100, None).unwrap(), events_before);
    hub.close().unwrap();

    raw(&place.path).execute_batch("DROP TRIGGER fail;").unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.child_today(&asha, at(8, 5)).unwrap(), before);
    // The device never had an answer, so it sends its slices again. They
    // count once.
    exchange(&mut hub, &mut pc, at(8, 6), true);
    exchange(&mut hub, &mut pc, at(8, 7), true);
    assert_eq!(hub.child_today(&asha, at(8, 7)).unwrap().used_secs, 5 * M);
    assert_eq!(hub.usage(&asha, day_of(at(8, 7)), day_of(at(8, 7))).unwrap().secs, 5 * M);
}

#[test]
fn reports_that_do_not_fit_are_refused_and_change_nothing() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let ravi = child(&mut hub, "Ravi").child_id;
    let mut pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 0));
    let mut tablet = pair(&mut hub, &ravi, "Tablet", DeviceKind::Portable, at(7, 0));
    pc.state.record_use("game", at(8, 0), at(8, 5));
    let snapshot = |hub: &Hub| (hub.family_today(at(8, 5)).unwrap(), hub.events(100, None).unwrap());
    let before = snapshot(&hub);

    // A device the family has never heard of.
    let mut report = pc.state.report(at(8, 5), true);
    report.device_id = "device-0000000000000000".into();
    let err = hub.handle_report(&report, at(8, 5)).unwrap_err();
    assert_eq!(err, HubError::UnknownDevice { device_id: report.device_id.clone() });
    plain(&err.to_string());

    // A report that names Ravi, from Asha's PC.
    let mut report = pc.state.report(at(8, 5), true);
    report.child_id = ravi.clone();
    assert_eq!(hub.handle_report(&report, at(8, 5)).unwrap_err(), HubError::DeviceOfAnotherChild {
        device_id: pc.id.clone()
    });
    // A report that names a child who does not exist.
    let mut report = pc.state.report(at(8, 5), true);
    report.child_id = "child-ffffffffffffffff".into();
    assert!(matches!(hub.handle_report(&report, at(8, 5)), Err(HubError::DeviceOfAnotherChild { .. })));
    assert_eq!(snapshot(&hub), before);

    // A removed device.
    hub.remove_device(&phone.id, at(8, 6)).unwrap();
    let report = phone.state.report(at(8, 7), true);
    let err = hub.handle_report(&report, at(8, 7)).unwrap_err();
    assert_eq!(err, HubError::DeviceRemoved { device_id: phone.id.clone() });
    plain(&err.to_string());

    // A removed child: its devices are gone with it.
    hub.remove_child(&ravi).unwrap();
    let report = tablet.state.report(at(8, 8), true);
    assert_eq!(hub.handle_report(&report, at(8, 8)).unwrap_err(), HubError::UnknownDevice {
        device_id: tablet.id.clone()
    });

    // Asha's PC itself is still answered.
    assert!(hub.handle_report(&pc.state.report(at(8, 9), true), at(8, 9)).is_ok());
}

#[test]
fn hostile_reports_neither_crash_nor_pollute_the_record() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let ravi = child(&mut hub, "Ravi").child_id;
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 0));
    let today = day_of(at(8, 0));
    let slice = |id: &str, app: &str, day: i64, start: i64, end: i64| Slice {
        id: format!("{}/{id}", phone.id),
        child_id: asha.clone(),
        device_id: phone.id.clone(),
        app_id: app.into(),
        day,
        start,
        end,
        counts: true,
    };
    let long_app = "x".repeat(10_000);
    let mut report: Report = phone.state.report(at(8, 0), true);
    report.message_number = u64::MAX;
    report.used_secs = u64::MAX;
    report.lease_total_secs = u64::MAX;
    report.away_cap_secs = u64::MAX;
    report.wrong_guesses = u32::MAX;
    report.sent_at = i64::MIN;
    report.slices = vec![
        slice("1", "fine", today, at(8, 0), at(8, 3)),
        slice("1", "fine", today, at(8, 0), at(8, 3)),
        slice("2", "forever", today, i64::MIN, i64::MAX),
        slice("3", "backwards", today, at(8, 5), at(8, 4)),
        slice("4", "far future", today + 10_000, at(8, 0), at(8, 1)),
        slice("5", "far past", i64::MIN, at(8, 0), at(8, 1)),
        slice("6", &long_app, today, at(8, 6), at(8, 7)),
        slice("7", "nul\0app", today, at(8, 7), at(8, 8)),
        Slice { child_id: ravi.clone(), ..slice("8", "someone else's", today, at(8, 0), at(8, 1)) },
        Slice { device_id: "device-1234".into(), ..slice("9", "another device", today, at(8, 0), at(8, 1)) },
    ];
    hub.handle_report(&report, at(8, 0)).unwrap();
    // The same report again changes nothing in the record.
    hub.handle_report(&report, at(8, 1)).unwrap();

    let usage = hub.usage(&asha, today - 1, today + 1).unwrap();
    let mut apps: Vec<(String, u64)> = usage.apps.iter().map(|a| (a.app_id.clone(), a.secs)).collect();
    apps.sort();
    let mut expected = vec![
        ("fine".to_string(), 3 * M),
        // A slice from the year dot to the end of time is kept at one day.
        ("forever".to_string(), 86_400),
        (long_app.clone(), M),
        ("nul\0app".to_string(), M),
    ];
    expected.sort();
    assert_eq!(apps, expected);
    assert!(hub.usage(&ravi, today - 1, today + 1).unwrap().apps.is_empty());
    // The hub still answers, the views still work, and a clock that far off
    // was noted once.
    let v = hub.child_today(&asha, at(8, 2)).unwrap();
    assert_eq!(v.devices[0].clock_off_by_secs, Some(i64::MIN.saturating_sub(at(8, 0))));
    let clock_events = hub
        .events(100, None)
        .unwrap()
        .into_iter()
        .filter(|e| matches!(e.what, EventKind::ClockFarOff { .. }))
        .count();
    assert_eq!(clock_events, 1);
    exchange(&mut hub, &mut phone, at(8, 30), true);
}

#[test]
fn new_rules_bring_every_device_without_them_into_the_wanted_list() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 0));
    exchange(&mut hub, &mut pc, at(8, 0), false);
    exchange(&mut hub, &mut phone, at(8, 0), false);
    assert!(hub.wanted_reports(at(8, 1)).unwrap().is_empty());

    let mut rules = hub.rules(&asha).unwrap();
    rules.lease_minutes = 5;
    hub.replace_rules(&asha, rules, at(9, 0)).unwrap();
    let wanted: BTreeSet<String> = hub.wanted_reports(at(9, 0)).unwrap().into_iter().collect();
    assert_eq!(wanted, [pc.id.clone(), phone.id.clone()].into());

    // The PC reports, takes the grant with the new rules, and reports
    // holding them.
    exchange(&mut hub, &mut pc, at(9, 1), false);
    assert_eq!(pc.state.rules_version(), 2);
    exchange(&mut hub, &mut pc, at(9, 2), false);
    assert_eq!(hub.wanted_reports(at(9, 2)).unwrap(), vec![phone.id.clone()]);
    // A removed device is never wanted.
    hub.remove_device(&phone.id, at(9, 3)).unwrap();
    assert!(hub.wanted_reports(at(9, 3)).unwrap().is_empty());
}

#[test]
fn in_use_follows_the_newest_report() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    let in_use = |hub: &Hub, now: i64| hub.child_today(&asha, now).unwrap().devices[0].in_use;
    exchange(&mut hub, &mut pc, at(8, 0), true);
    assert!(!in_use(&hub, at(8, 0)), "a report with no use");
    use_app(&mut hub, &mut pc, "game", at(8, 0), at(8, 5));
    assert!(in_use(&hub, at(8, 5)));
    assert!(in_use(&hub, at(8, 15)), "ten minutes after the report");
    assert!(!in_use(&hub, at(8, 16)), "heard from too long ago");
    // A report whose use ended more than two minutes before it was sent.
    pc.state.record_use("game", at(8, 20), at(8, 22));
    exchange(&mut hub, &mut pc, at(8, 25), false);
    assert!(!in_use(&hub, at(8, 25)));
    use_app(&mut hub, &mut pc, "game", at(8, 25), at(8, 26));
    assert!(in_use(&hub, at(8, 26)));
    // An idle report after it says it is no longer in use.
    exchange(&mut hub, &mut pc, at(8, 30), false);
    assert!(!in_use(&hub, at(8, 30)));
}

#[test]
fn a_clock_far_off_is_noted_once_until_it_is_right_again() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 0));
    let clock_events = |hub: &Hub| {
        hub.events(100, None).unwrap().into_iter().filter(|e| matches!(e.what, EventKind::ClockFarOff { .. })).count()
    };
    let off_by = |hub: &Hub| hub.child_today(&asha, at(9, 0)).unwrap().devices[0].clock_off_by_secs;

    // The phone's clock is an hour ahead.
    for minute in [0, 1, 2] {
        let device_clock = at(9, minute) + HOUR;
        let report = phone.state.report(device_clock, false);
        phone.state.take_grant(&hub.handle_report(&report, at(9, minute)).unwrap(), device_clock);
    }
    assert_eq!(clock_events(&hub), 1);
    assert_eq!(off_by(&hub), Some(HOUR));
    // Four minutes off is close enough.
    let report = phone.state.report(at(9, 0) + 4 * MIN, false);
    hub.handle_report(&report, at(9, 0)).unwrap();
    assert_eq!(off_by(&hub), None);
    // Off again: noted again, behind this time.
    let report = phone.state.report(at(9, 30) + HOUR, false);
    hub.handle_report(&report, at(11, 0)).unwrap();
    assert_eq!(clock_events(&hub), 2);
    let events = hub.events(1, None).unwrap();
    assert_eq!(events[0].what, EventKind::ClockFarOff { device_id: phone.id.clone(), off_by_secs: -(30 * MIN) });
}
