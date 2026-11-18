//! Inputs a careless or hostile caller might send: every one is refused in
//! plain words or taken safely, and nothing panics.

use limo_core::{CodeError, DeviceKind, MasterError, RequestReply};
use limo_hub::{AccountLink, AgeBand, Hub, HubError, MAX_EVENTS_PER_CALL, TextField};

use crate::common::*;

#[test]
fn names_and_labels_are_checked_and_odd_ones_kept_as_typed() {
    let place = place();
    let mut hub = new_family(&place.path);
    let long = "A".repeat(10_000);
    let cases: Vec<(&str, HubError)> = vec![
        (&long, HubError::TextTooLong { field: TextField::ChildName, max_chars: 40 }),
        ("   ", HubError::TextEmpty { field: TextField::ChildName }),
        ("As\0ha", HubError::TextHasHiddenCharacters { field: TextField::ChildName }),
        ("Asha\u{202E}ahsA", HubError::TextHasHiddenCharacters { field: TextField::ChildName }),
    ];
    for (name, expected) in cases {
        let err = hub.add_child(name, AgeBand::UnderSix, IST, at(7, 0)).unwrap_err();
        assert_eq!(err, expected);
        plain(&err.to_string());
    }
    let odd = ["आशा", "Zoë 🙂", "Robert'); DROP TABLE child;--", "  trimmed  "];
    let ids: Vec<String> =
        odd.iter().map(|n| hub.add_child(n, AgeBand::UnderSix, IST, at(7, 0)).unwrap().child_id).collect();
    // No two children share a name, whatever the case.
    let err = hub.add_child("ZOË 🙂", AgeBand::UnderSix, IST, at(7, 0)).unwrap_err();
    assert_eq!(err, HubError::ChildNameTaken { name: "ZOË 🙂".into() });
    plain(&err.to_string());
    assert!(matches!(hub.rename_child(&ids[0], "trimmed"), Err(HubError::ChildNameTaken { .. })));
    assert_eq!(hub.rename_child(&ids[1], "zoë 🙂").unwrap().name, "zoë 🙂");
    hub.close().unwrap();

    let mut hub = Hub::open(&place.path).unwrap();
    let names: Vec<String> = hub.children().unwrap().into_iter().map(|c| c.name).collect();
    assert_eq!(names, vec!["आशा", "zoë 🙂", "Robert'); DROP TABLE child;--", "trimmed"]);
    let a = &ids[0];
    assert!(matches!(
        hub.pair_device(a, &"d".repeat(41), DeviceKind::Portable, "android", at(7, 0)),
        Err(HubError::TextTooLong { field: TextField::DeviceName, .. })
    ));
    assert!(matches!(
        hub.pair_device(a, "Phone", DeviceKind::Portable, "", at(7, 0)),
        Err(HubError::TextEmpty { field: TextField::Platform })
    ));
    assert!(matches!(
        hub.link_account(&"s".repeat(201), "Mum", AccountLink::NotLimited),
        Err(HubError::TextTooLong { field: TextField::AccountId, .. })
    ));
    assert!(matches!(
        hub.link_account("S-1-5-21-1000", "Mum\nDad", AccountLink::NotLimited),
        Err(HubError::TextHasHiddenCharacters { field: TextField::AccountName })
    ));
    assert!(matches!(
        hub.give_extra(a, 10, &"t".repeat(101), at(8, 0)),
        Err(HubError::TextTooLong { field: TextField::TapId, .. })
    ));
}

#[test]
fn unknown_ids_are_refused_plainly() {
    let place = place();
    let mut hub = new_family(&place.path);
    let nobody = "child-0000000000000000";
    let unknown = HubError::UnknownChild { child_id: nobody.into() };
    let t = at(8, 0);
    let today = day_of(t);
    assert_eq!(hub.child(nobody).unwrap_err(), unknown);
    assert_eq!(hub.rules(nobody).unwrap_err(), unknown);
    assert_eq!(hub.replace_rules(nobody, limo_core::RuleSet::new(1, IST), t).unwrap_err(), unknown);
    assert_eq!(hub.child_today(nobody, t).unwrap_err(), unknown);
    assert_eq!(hub.usage(nobody, today, today).unwrap_err(), unknown);
    assert_eq!(hub.devices(nobody).unwrap_err(), unknown);
    assert_eq!(hub.pair_device(nobody, "Phone", DeviceKind::Portable, "android", t).unwrap_err(), unknown);
    assert_eq!(hub.give_extra(nobody, 10, "", t).unwrap_err(), unknown);
    assert_eq!(hub.extra_time_today(nobody, t).unwrap_err(), unknown);
    assert_eq!(hub.lock_now(nobody, None, t).unwrap_err(), unknown);
    assert_eq!(hub.unlock_until(nobody, t + HOUR, t).unwrap_err(), unknown);
    assert_eq!(hub.lift(nobody, t).unwrap_err(), unknown);
    assert_eq!(hub.answer_request(nobody, "x", RequestReply::Refuse, t).unwrap_err(), unknown);
    assert_eq!(hub.rename_child(nobody, "Asha").unwrap_err(), unknown);
    assert_eq!(hub.set_age_band(nobody, AgeBand::UnderSix).unwrap_err(), unknown);
    assert_eq!(hub.remove_child(nobody).unwrap_err(), unknown);
    assert_eq!(
        hub.link_account("S-1", "Kid", AccountLink::Child { child_id: nobody.into() }).unwrap_err(),
        unknown
    );
    plain(&unknown.to_string());

    let no_device = "device-0000000000000000";
    let unknown = HubError::UnknownDevice { device_id: no_device.into() };
    assert_eq!(hub.make_code(no_device, 15, t).unwrap_err(), unknown);
    assert_eq!(hub.remove_device(no_device, t).unwrap_err(), unknown);
    assert_eq!(hub.rename_device(no_device, "x").unwrap_err(), unknown);
    assert_eq!(hub.device(no_device).unwrap_err(), unknown);
    assert_eq!(hub.unlink_account("S-404").unwrap_err(), HubError::UnknownAccount { account_id: "S-404".into() });
    plain(&HubError::UnknownAccount { account_id: "S-404".into() }.to_string());
}

#[test]
fn numbers_out_of_range_are_refused_in_the_cores_words() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    let t = at(8, 0);
    let mut errors = Vec::new();

    for minutes in [0, 1_441, u32::MAX] {
        let err = hub.give_extra(&asha, minutes, "", t).unwrap_err();
        assert_eq!(err, HubError::Refused { problem: MasterError::MinutesOutOfRange { minutes } });
        errors.push(err);
    }
    let err = hub.make_code(&pc.id, 20, t).unwrap_err();
    assert_eq!(err, HubError::Code { problem: CodeError::MinutesNotOffered { minutes: 20 } });
    errors.push(err);
    for n in 0..10 {
        assert_eq!(hub.make_code(&pc.id, [15, 30, 60][n % 3], t).unwrap().counter, n as u8);
    }
    let err = hub.make_code(&pc.id, 15, t).unwrap_err();
    assert!(matches!(err, HubError::Code { problem: CodeError::AllCodesMade { .. } }));
    errors.push(err);

    for err in [
        hub.lock_now(&asha, Some(t - 1), t).unwrap_err(),
        hub.unlock_until(&asha, t + 25 * HOUR, t).unwrap_err(),
        hub.unlock_until(&asha, t, t).unwrap_err(),
    ] {
        assert_eq!(err, HubError::Refused { problem: MasterError::TimeOutOfRange });
        errors.push(err);
    }
    // A PIN is 6 to 8 digits. Four and five digits are refused by the hub
    // itself, in its own words, before the core (which refuses them too).
    for pin in ["1234", "12345", "12a456", "123456789", " 123456", ""] {
        let err = hub.set_pin(pin, t).unwrap_err();
        assert_eq!(err, HubError::PinWrongShape { min_digits: 6, max_digits: 8 }, "{pin:?}");
        errors.push(err);
    }
    assert!(!hub.has_pin().unwrap());
    hub.set_pin("246810", t).unwrap();
    hub.set_pin("24681012", t).unwrap();
    assert!(hub.has_pin().unwrap());
    let err = hub.pair_device(&asha, "Second PC", DeviceKind::OnMaster, "windows", t).unwrap_err();
    assert_eq!(err, HubError::MasterPcAlreadyPaired);
    errors.push(err);

    // Rules the core refuses come back with its problems, unchanged.
    let mut bad = hub.rules(&asha).unwrap();
    bad.lease_minutes = 0;
    bad.away_share_percent = 150;
    bad.blocked_apps = vec!["".into()];
    let mut as_checked = bad.clone();
    as_checked.version = 2;
    let err = hub.replace_rules(&asha, bad, t).unwrap_err();
    assert_eq!(err, HubError::InvalidRules { problems: as_checked.validate() });
    assert_eq!(hub.rules(&asha).unwrap().version, 1);
    errors.push(err);
    let err = hub.add_child("Far away", AgeBand::UnderSix, 20 * 3_600, t).unwrap_err();
    assert!(matches!(err, HubError::InvalidRules { .. }));
    errors.push(err);

    // Ranges of days: backwards, too long, or at the ends of the numbers.
    let today = day_of(t);
    for (from, to) in [(today, today - 1), (today - 400, today), (i64::MIN, i64::MAX), (i64::MAX, i64::MIN)] {
        let err = hub.usage(&asha, from, to).unwrap_err();
        assert_eq!(err, HubError::DaysOutOfRange { most: 400 });
        errors.push(err);
    }
    assert!(hub.usage(&asha, today - 399, today).is_ok());
    assert!(hub.usage(&asha, i64::MAX - 5, i64::MAX).is_ok());
    assert!(hub.events(u32::MAX, None).unwrap().len() <= MAX_EVENTS_PER_CALL as usize);
    assert!(hub.events(10, Some(i64::MIN)).unwrap().is_empty());

    for err in &errors {
        plain(&err.to_string());
    }
}
