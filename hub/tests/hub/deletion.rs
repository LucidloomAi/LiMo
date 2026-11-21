//! Deleted means deleted; removed devices hold nothing; a change that fails
//! part way leaves nothing.

use limo_core::{DeviceKind, RequestReply};
use limo_hub::{AccountLink, AccountStanding, EventKind, Hub, HubError, PasswordOutcome, PhoneApp, PhoneNote};

use crate::common::*;

const M: u64 = 60;
/// A name that appears nowhere else, so that the file can be searched for
/// it.
const ZEPH: &str = "Zephyrine";
const ZEPH_PHONE: &str = "Quixotic handset";
/// The link key of Zephyrine's phone paired with an offer, and of Ravi's.
const ZEPH_KEY: u8 = 41;
const RAVI_KEY: u8 = 77;
/// The mark in the text of an offer for Zephyrine that was never used.
const ZEPH_OFFER: &str = "ZEBRAOFFERSECRET";

/// Zephyrine with a PC and a phone, a day of use folded into totals and a
/// day in detail, a request, codes, an account and events, and a second
/// phone paired with an offer: its key, its notes, its apps, and an offer
/// not yet used; and Ravi, with a phone of his own paired with an offer.
fn two_children(hub: &mut Hub) -> (String, String, String) {
    let zeph = child(hub, ZEPH).child_id;
    let ravi = child(hub, "Ravi").child_id;
    let mut pc = pair(hub, &zeph, "Zeph PC", DeviceKind::OnMaster, at(7, 0));
    let mut phone = pair(hub, &zeph, ZEPH_PHONE, DeviceKind::Portable, at(7, 0));
    let mut tablet = pair(hub, &ravi, "Ravi tablet", DeviceKind::Portable, at(7, 0));
    let mut linked = pair_phone(hub, &zeph, "Zebra link phone", ZEPH_KEY, at(7, 0));
    pair_phone(hub, &ravi, "Ravi link phone", RAVI_KEY, at(7, 0));
    let (id, text) = offer_parts(ZEPH_KEY + 1, ZEPH_OFFER);
    hub.keep_offer(&zeph, &id, &text, at(7, 10), at(7, 0)).unwrap();
    let apps = [PhoneApp { package: "com.zebra.game".into(), label: "Zebra Galaxy Game".into() }];
    hub.keep_phone_apps(&linked.id, &apps, "abcdef", at(7, 1)).unwrap();
    let note = PhoneNote::ClockMoved { by_secs: 7_777_777, followed: false, at: at(7, 2) };
    let report = report_with_notes(&mut linked, &[note], 1, at(7, 2));
    let grant = hub.handle_report(&report, at(7, 2)).unwrap();
    linked.state.take_grant(&grant, at(7, 2));
    exchange(hub, &mut pc, at(8, 0), true);
    use_app(hub, &mut pc, "zebra-game", at(8, 0), at(8, 9));
    exchange(hub, &mut phone, at(8, 10), true);
    // Many short stretches, so that the tables span many pages of the file.
    for i in 0..400 {
        let t = at(8, 10) + i;
        phone.state.record_use(if i % 2 == 0 { "zebra-video" } else { "zebra-chat" }, t, t + 1);
        tablet.state.record_use("drawing", t, t + 1);
    }
    exchange(hub, &mut phone, at(8, 20), true);
    exchange(hub, &mut tablet, at(8, 20), true);
    phone.state.ask_for_time(10, "zebra homework", at(9, 0)).unwrap();
    exchange(hub, &mut phone, at(9, 0), false);
    hub.link_account("S-1-5-21-2002", "zeph-account", AccountLink::Child { child_id: zeph.clone() }).unwrap();
    hub.lock_now(&zeph, Some(at(10, 0)), at(9, 10)).unwrap();
    // With a detail window of a week, a week on, today folds into totals.
    let week = 7 * 86_400;
    hub.set_detail_days(7).unwrap();
    use_app(hub, &mut pc, "zebra-game", at(8, 0) + week, at(8, 3) + week);
    hub.tidy(at(9, 0) + week).unwrap();
    hub.make_code(&phone.id, 15, at(9, 5) + week).unwrap();
    (zeph, ravi, phone.id)
}

#[test]
fn removing_a_child_removes_every_row_and_overwrites_it() {
    let place = place();
    let mut hub = new_family(&place.path);
    let (zeph, ravi, _) = two_children(&mut hub);
    hub.close().unwrap();
    {
        let conn = raw(&place.path);
        let mine = format!("child_id = '{zeph}'");
        assert!(count(&conn, "day_total", &mine) > 0, "the story folded a day");
        assert!(count(&conn, "slice", &mine) > 0);
        assert!(count(&conn, "code_made", "1") > 0);
        assert!(count(&conn, "event", &mine) > 0);
    }
    assert!(file_holds(&place.path, ZEPH) && file_holds(&place.path, ZEPH_PHONE));

    let mut hub = Hub::open(&place.path).unwrap();
    let ravi_before = hub.child_today(&ravi, at(12, 0)).unwrap();
    hub.remove_child(&zeph).unwrap();
    assert_eq!(hub.child(&zeph).unwrap_err(), HubError::UnknownChild { child_id: zeph.clone() });
    assert_eq!(hub.children().unwrap().len(), 1);
    assert_eq!(hub.account_standing("S-1-5-21-2002").unwrap(), AccountStanding::NotLinked);
    assert!(hub.waiting_requests(at(9, 1)).unwrap().is_empty());
    assert_eq!(hub.child_today(&ravi, at(12, 0)).unwrap(), ravi_before);
    hub.close().unwrap();

    let conn = raw(&place.path);
    let mine = format!("child_id = '{zeph}'");
    for table in ["child", "book", "device", "slice", "day_total", "account", "event", "offer", "phone", "phone_apps"] {
        let condition = if table == "child" { format!("id = '{zeph}'") } else { mine.clone() };
        assert_eq!(count(&conn, table, &condition), 0, "{table}");
    }
    assert_eq!(count(&conn, "code_made", "1"), 0);
    assert_eq!(count(&conn, "child", "1"), 1);
    // Ravi's usage of that day was folded into totals, and stays; so do his
    // phone and its key.
    assert!(count(&conn, "day_total", &format!("child_id = '{ravi}'")) > 0);
    assert_eq!(count(&conn, "phone", &format!("child_id = '{ravi}'")), 2);
    drop(conn);
    // Not only gone from the tables: gone from the file.
    let link = ["com.zebra.game", "Zebra Galaxy Game", "Zebra link phone", ZEPH_OFFER, "7777777"];
    for needle in [ZEPH, ZEPH_PHONE, "zebra-game", "zebra-video", "zebra homework", "zeph-account", zeph.as_str()]
        .into_iter()
        .chain(link)
    {
        assert!(!file_holds(&place.path, needle), "{needle} is still in the file");
    }
    assert!(!file_holds_bytes(&place.path, &link_key(ZEPH_KEY)), "the phone's link key is still in the file");
    // What stays is found by the same search.
    assert!(file_holds_bytes(&place.path, &link_key(RAVI_KEY)) && file_holds(&place.path, "Ravi link phone"));
    let master_key = Hub::open(&place.path).unwrap().master_key().unwrap();
    assert!(file_holds_bytes(&place.path, master_key.expose_private()));
}

#[test]
fn deleting_the_family_leaves_an_empty_valid_store() {
    let place = place();
    let mut hub = new_family(&place.path);
    two_children(&mut hub);
    hub.set_pin("482915", at(11, 0)).unwrap();
    hub.link_account("S-1-5-21-1000", "Mumtaz", AccountLink::NotLimited).unwrap();
    let master_key = hub.master_key().unwrap();
    hub.close().unwrap();
    assert!(file_holds_bytes(&place.path, master_key.expose_private()), "the search finds the key while it is there");
    let mut hub = Hub::open(&place.path).unwrap();
    hub.delete_family().unwrap();

    assert!(!hub.has_family().unwrap());
    assert!(hub.children().unwrap().is_empty());
    assert!(hub.accounts().unwrap().is_empty());
    assert!(hub.events(100, None).unwrap().is_empty());
    assert_eq!(hub.detail_days().unwrap(), limo_hub::DEFAULT_DETAIL_DAYS);
    assert_eq!(hub.has_pin().unwrap_err(), HubError::NoFamily);
    assert_eq!(hub.check_password(PASSWORD, at(12, 0)).unwrap_err(), HubError::NoFamily);
    assert_eq!(hub.add_child("Asha", limo_hub::AgeBand::UnderSix, IST, at(12, 0)).unwrap_err(), HubError::NoFamily);
    hub.close().unwrap();

    let conn = raw(&place.path);
    for table in [
        "family", "child", "book", "device", "slice", "day_total", "code_made", "account", "event", "offer", "phone",
        "phone_apps",
    ] {
        assert_eq!(count(&conn, table, "1"), 0, "{table}");
    }
    drop(conn);
    for needle in [ZEPH, ZEPH_PHONE, "Ravi tablet", "drawing", "zebra-game", "Mumtaz", "com.zebra.game", ZEPH_OFFER] {
        assert!(!file_holds(&place.path, needle), "{needle} is still in the file");
    }
    // The keys too: the master's own, and every phone's.
    let keys = [
        ("master", *master_key.expose_private()),
        ("Zephyrine's phone", link_key(ZEPH_KEY)),
        ("Ravi's phone", link_key(RAVI_KEY)),
    ];
    for (what, key) in keys {
        assert!(!file_holds_bytes(&place.path, &key), "the {what} key is still in the file");
    }

    // A new family can start in the same file, with a new password.
    let mut hub = Hub::open(&place.path).unwrap();
    hub.create_family("a brand new password", at(12, 0)).unwrap();
    assert_eq!(hub.check_password(PASSWORD, at(12, 1)).unwrap(), PasswordOutcome::Wrong {
        wrong_guesses: 1,
        wait_until: None
    });
    assert!(!hub.has_pin().unwrap());
    let asha = child(&mut hub, "Asha");
    assert_eq!(hub.child_today(&asha.child_id, at(12, 2)).unwrap().used_secs, 0);
}

#[test]
fn a_removed_device_keeps_its_use_and_holds_back_nothing() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    let mut phone = pair(&mut hub, &asha, "Old phone", DeviceKind::Portable, at(7, 0));
    exchange(&mut hub, &mut phone, at(8, 0), true);
    use_app(&mut hub, &mut phone, "video", at(8, 0), at(8, 20));
    let before = hub.child_today(&asha, at(8, 30)).unwrap();
    assert_eq!(before.used_secs, 20 * M);
    let held_before: u64 = before.devices.iter().map(|d| d.holds_secs.unwrap()).sum();
    assert_eq!(held_before, 60 * M, "the phone's lease is spent; 50 minutes of away time and 10 of lease are held");

    hub.remove_device(&phone.id, at(8, 30)).unwrap();
    let after = hub.child_today(&asha, at(8, 30)).unwrap();
    assert_eq!(after.used_secs, 20 * M, "its use still counts today");
    assert_eq!(after.devices.len(), 1);
    assert_eq!(after.devices[0].holds_secs, Some(0));
    assert_eq!(hub.devices(&asha).unwrap().len(), 1);
    // The PC now gets the time the phone held.
    let grant = exchange(&mut hub, &mut pc, at(8, 31), true);
    assert_eq!(grant.left_for_you_secs, Some(100 * M));
    // Its history stays, marked as removed.
    let usage = hub.usage(&asha, day_of(at(8, 0)), day_of(at(8, 0))).unwrap();
    assert_eq!(usage.devices.len(), 1);
    assert_eq!((usage.devices[0].name.as_str(), usage.devices[0].removed), ("Old phone", true));
    // It can do nothing more.
    assert_eq!(hub.make_code(&phone.id, 15, at(8, 32)).unwrap_err(), HubError::DeviceRemoved {
        device_id: phone.id.clone()
    });
    assert_eq!(hub.remove_device(&phone.id, at(8, 32)).unwrap_err(), HubError::DeviceRemoved {
        device_id: phone.id.clone()
    });
    assert!(matches!(hub.events(1, None).unwrap()[0].what, EventKind::DeviceRemoved { .. }));
    // Moving it to another child is pairing it again: a new id and secret.
    let ravi = child(&mut hub, "Ravi").child_id;
    let again = hub.pair_device(&ravi, "Old phone", DeviceKind::Portable, "android", at(9, 0)).unwrap();
    assert_ne!(again.device_id, phone.id);
    assert_eq!(hub.devices(&ravi).unwrap().len(), 1);
}

#[test]
fn a_removed_phone_frees_its_time_and_drops_the_code_time_it_did_not_use() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 0));
    // A 30 minute code on the phone, which holds no lease: it spends 10
    // minutes of the code's time and reports.
    let code = hub.make_code(&phone.id, 30, at(9, 0)).unwrap();
    assert!(matches!(phone.state.try_code(&code.shown, at(9, 0)), limo_core::TryOutcome::Added { .. }));
    phone.state.record_use("game", at(9, 0), at(9, 10));
    exchange(&mut hub, &mut phone, at(9, 10), false);
    let v = hub.child_today(&asha, at(9, 10)).unwrap();
    assert_eq!((v.extra_secs, v.used_secs), (30 * M, 10 * M));
    // Held back for the phone: its 60 minutes away, and the 20 code minutes
    // left. The PC can have the other 60.
    assert_eq!(v.devices[1].holds_secs, Some(80 * M));
    assert_eq!(exchange(&mut hub, &mut pc, at(9, 11), false).left_for_you_secs, Some(60 * M));

    hub.remove_device(&phone.id, at(9, 30)).unwrap();
    let v = hub.child_today(&asha, at(9, 30)).unwrap();
    // The 10 code minutes it used stay counted and given; the 20 it did not
    // use are dropped, not passed to the PC; its away time is free. Nothing
    // is held back for it any more, the used part included.
    assert_eq!((v.extra_secs, v.used_secs, v.left_secs), (10 * M, 10 * M, Some(120 * M)));
    assert_eq!(exchange(&mut hub, &mut pc, at(9, 31), false).left_for_you_secs, Some(120 * M));
    let pieces = hub.extra_time_today(&asha, at(9, 31)).unwrap();
    assert_eq!((pieces[0].secs, pieces[0].dropped_secs), (30 * M, 20 * M));
}

#[test]
fn a_change_that_fails_part_way_leaves_nothing_behind() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let ravi = child(&mut hub, "Ravi").child_id;
    let mut pc = pair(&mut hub, &asha, "PC", DeviceKind::OnMaster, at(7, 0));
    let before = (hub.family_today(at(8, 0)).unwrap(), hub.events(100, None).unwrap());
    hub.close().unwrap();

    // Pairing fails at its last step, recording the event: the book had
    // already taken the device, and the device row was written.
    raw(&place.path)
        .execute_batch("CREATE TRIGGER fail BEFORE INSERT ON event BEGIN SELECT RAISE(ABORT, 'test failure'); END;")
        .unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    let err = hub.pair_device(&asha, "Phone", DeviceKind::Portable, "android", at(8, 0)).unwrap_err();
    assert!(matches!(err, HubError::Storage { .. }), "{err:?}");
    assert!(hub.lock_now(&asha, None, at(8, 0)).is_err());
    let rules = hub.rules(&asha).unwrap();
    assert!(hub.replace_rules(&asha, rules, at(8, 0)).is_err());
    assert_eq!((hub.family_today(at(8, 0)).unwrap(), hub.events(100, None).unwrap()), before);
    assert_eq!(hub.rules(&asha).unwrap().version, 1);
    hub.close().unwrap();
    raw(&place.path).execute_batch("DROP TRIGGER fail;").unwrap();
    {
        let conn = raw(&place.path);
        assert_eq!(count(&conn, "device", "1"), 1);
    }

    // The PIN is set on every child's book in one transaction. With Ravi's
    // book damaged, Asha's must not take the PIN either.
    raw(&place.path).execute("UPDATE book SET json = '{' WHERE child_id = ?1", [&ravi]).unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    let err = hub.set_pin("482915", at(9, 0)).unwrap_err();
    assert!(matches!(err, HubError::Damaged { .. }), "{err:?}");
    plain(&err.to_string());
    assert!(!hub.has_pin().unwrap());
    let grant = exchange(&mut hub, &mut pc, at(9, 1), false);
    assert_eq!(grant.pin, None, "Asha's book has no PIN");
    // The damaged child is refused plainly; the other carries on.
    assert!(matches!(hub.child_today(&ravi, at(9, 2)), Err(HubError::Damaged { .. })));
    assert!(hub.child_today(&asha, at(9, 2)).is_ok());
    // Answering a request that does not exist is refused by the core.
    let err = hub.answer_request(&asha, "nope", RequestReply::Refuse, at(9, 3)).unwrap_err();
    assert!(matches!(err, HubError::Refused { .. }));
    plain(&err.to_string());
}
