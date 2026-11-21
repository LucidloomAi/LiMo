//! What the link keeps in the hub: the master's own key, the pairing
//! offers, a phone paired with its key in one transaction, the phones the
//! link may meet, and a removed phone told so.

use limo_core::{DeviceKind, DeviceState};
use limo_hub::{EventKind, Hub, HubError, LinkPhone, OFFER_KEEP_SECS, PhoneForm, PhoneRequest};

use crate::common::*;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn the_master_key_is_made_with_the_family_and_kept_for_its_life() {
    let place = place();
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.master_key().unwrap_err(), HubError::NoFamily);
    hub.create_family(PASSWORD, at(7, 0)).unwrap();
    let key = hub.master_key().unwrap();
    assert_ne!(key.expose_private(), &[0u8; 32]);
    hub.close().unwrap();

    // The same after a restart; another family has another.
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.master_key().unwrap(), key);
    let elsewhere = crate::common::place();
    assert_ne!(new_family(&elsewhere.path).master_key().unwrap(), key);
    // Never shown.
    let shown = format!("{key:?} {hub:?}");
    assert_eq!(format!("{key:?}"), "MasterKey(hidden)");
    assert!(!shown.contains(&hex(key.expose_private())[..16]), "{shown}");
    assert!(!shown.contains(&format!("{:?}", &key.expose_private()[..4])[1..10]), "{shown}");

    // It goes with the family, and a new family gets a new one.
    hub.delete_family().unwrap();
    assert_eq!(hub.master_key().unwrap_err(), HubError::NoFamily);
    hub.create_family(PASSWORD, at(8, 0)).unwrap();
    assert_ne!(hub.master_key().unwrap(), key);
}

#[test]
fn an_offer_is_kept_for_a_child_and_read_back_whole() {
    let place = place();
    let mut hub = Hub::open(&place.path).unwrap();
    let (id, text) = offer_parts(1, "SECRET");
    assert_eq!(hub.keep_offer("child-x", &id, &text, at(7, 10), at(7, 0)).unwrap_err(), HubError::NoFamily);
    hub.create_family(PASSWORD, at(7, 0)).unwrap();
    let asha = child(&mut hub, "Asha").child_id;

    // The link's id in either case; kept in lowercase.
    let kept = hub.keep_offer(&asha, &id.to_uppercase(), &text, at(7, 10), at(7, 0)).unwrap();
    assert_eq!((kept.offer_id.as_str(), kept.child_id.as_str(), kept.text.as_str()), (id.as_str(), asha.as_str(), text.as_str()));
    assert_eq!((kept.made_at, kept.expires_at, kept.used_at, kept.device_id.clone()), (at(7, 0), at(7, 10), None, None));
    assert!(!kept.is_used() && !kept.is_expired(at(7, 10)) && kept.is_expired(at(7, 10) + 1));
    hub.close().unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.offer(&id).unwrap(), Some(kept.clone()));
    assert_eq!(hub.offer("0000000000000000").unwrap(), None);
    assert_eq!(hub.offer("not an id").unwrap(), None);

    // What cannot be an offer is refused, and nothing is kept.
    let (id2, text2) = offer_parts(2, "OTHER");
    let refused = [
        hub.keep_offer(&asha, &id, &text2, at(7, 10), at(7, 0)).unwrap_err(),
        hub.keep_offer(&asha, "12345", &text2, at(7, 10), at(7, 0)).unwrap_err(),
        hub.keep_offer(&asha, "zzzzzzzzzzzzzzzz", &text2, at(7, 10), at(7, 0)).unwrap_err(),
        hub.keep_offer(&asha, &id2, "limo1: SPACE", at(7, 10), at(7, 0)).unwrap_err(),
        hub.keep_offer(&asha, &id2, "", at(7, 10), at(7, 0)).unwrap_err(),
        hub.keep_offer(&asha, &id2, &"A".repeat(513), at(7, 10), at(7, 0)).unwrap_err(),
        hub.keep_offer(&asha, &id2, &text2, at(6, 59), at(7, 0)).unwrap_err(),
        hub.keep_offer(&asha, &id2, &text2, at(7, 0) + 86_401, at(7, 0)).unwrap_err(),
    ];
    for err in &refused {
        assert_eq!(err, &HubError::BadOffer);
        plain(&err.to_string());
    }
    let err = hub.keep_offer("child-nobody", &id2, &text2, at(7, 10), at(7, 0)).unwrap_err();
    assert!(matches!(err, HubError::UnknownChild { .. }));
    assert_eq!(hub.offer(&id2).unwrap(), None);

    // Its text holds the secret, so it is never shown.
    let shown = format!("{kept:?}");
    assert!(shown.contains(&id) && !shown.contains(&text) && !shown.contains("SECRET"), "{shown}");
}

#[test]
fn a_phone_is_paired_its_key_kept_and_the_offer_used_in_one_step() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let offer = offer(&mut hub, &asha, 1, at(7, 0));
    let revision = hub.book_revision(&asha).unwrap();

    let pairing = hub.pair_device_with_offer(&offer.offer_id, &link_key(1), &request("Asha's Galaxy"), at(7, 5)).unwrap();
    assert_eq!(pairing.child_id, asha);
    assert!(hub.book_revision(&asha).unwrap() > revision, "the book took the phone");
    let used = hub.offer(&offer.offer_id).unwrap().unwrap();
    assert_eq!((used.used_at, used.device_id.as_deref()), (Some(at(7, 5)), Some(pairing.device_id.as_str())));
    assert_eq!(hub.link_phones().unwrap(), vec![LinkPhone {
        device_id: pairing.device_id.clone(),
        child_id: asha.clone(),
        link_key: link_key(1),
        removed: false
    }]);
    let device = hub.device(&pairing.device_id).unwrap();
    assert_eq!((device.name.as_str(), device.kind, device.platform.as_str()), ("Asha's Galaxy", DeviceKind::Portable, "android"));
    let phone = hub.phone(&pairing.device_id).unwrap();
    assert_eq!((phone.form, phone.android_version, phone.has_link_key), (PhoneForm::Child, Some(33), true));
    assert_eq!((phone.setup_finished_at, phone.apps, phone.removed_at, phone.released_at), (None, None, None, None));
    assert!(matches!(&hub.events(1, None).unwrap()[0].what, EventKind::DevicePaired { name, .. } if name == "Asha's Galaxy"));

    // The phone makes its state from the pairing and goes on like any other.
    let mut phone = Device { id: pairing.device_id.clone(), state: DeviceState::from_pairing(&pairing.welcome, at(7, 5)) };
    phone.state.set_code_secret(pairing.code_secret.clone()).unwrap();
    use_app(&mut hub, &mut phone, "com.google.android.youtube", at(8, 0), at(8, 10));
    assert_eq!(hub.child_today(&asha, at(8, 10)).unwrap().used_secs, 10 * MIN as u64);
    let code = hub.make_code(&phone.id, 15, at(8, 11)).unwrap();
    assert!(matches!(phone.state.try_code(&code.shown, at(8, 11)), limo_core::TryOutcome::Added { .. }));

    // The offer works once: a second phone with it is refused, and nothing
    // changes.
    let before = (hub.family_today(at(8, 12)).unwrap(), hub.events(100, None).unwrap(), hub.link_phones().unwrap());
    let err = hub.pair_device_with_offer(&offer.offer_id, &link_key(2), &request("Another"), at(7, 6)).unwrap_err();
    assert_eq!(err, HubError::OfferUsed);
    assert_eq!((hub.family_today(at(8, 12)).unwrap(), hub.events(100, None).unwrap(), hub.link_phones().unwrap()), before);
    plain(&err.to_string());
}

#[test]
fn an_offer_unknown_expired_or_for_a_guardian_or_a_known_key_pairs_nothing() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    pair_phone(&mut hub, &asha, "First", 1, at(7, 0));
    let offer = offer(&mut hub, &asha, 2, at(8, 0));
    let before = (hub.family_today(at(8, 0)).unwrap(), hub.events(100, None).unwrap(), hub.link_phones().unwrap());
    let unchanged = |hub: &Hub| {
        assert_eq!((hub.family_today(at(8, 0)).unwrap(), hub.events(100, None).unwrap(), hub.link_phones().unwrap()), before);
        assert!(!hub.offer(&offer.offer_id).unwrap().unwrap().is_used());
    };

    let unknown = hub.pair_device_with_offer("0123456789abcdef", &link_key(2), &request("Phone"), at(8, 1)).unwrap_err();
    assert_eq!(unknown, HubError::UnknownOffer);
    let late = hub.pair_device_with_offer(&offer.offer_id, &link_key(2), &request("Phone"), at(8, 10) + 1).unwrap_err();
    assert_eq!(late, HubError::OfferExpired);
    let guardian = PhoneRequest { form: PhoneForm::Guardian, ..request("Mira's phone") };
    let refused = hub.pair_device_with_offer(&offer.offer_id, &link_key(3), &guardian, at(8, 1)).unwrap_err();
    assert_eq!(refused, HubError::GuardianPhone);
    let known = hub.pair_device_with_offer(&offer.offer_id, &link_key(1), &request("Phone"), at(8, 1)).unwrap_err();
    assert_eq!(known, HubError::LinkKeyKnown);
    let platform = PhoneRequest { platform: String::new(), ..request("Phone") };
    assert!(matches!(
        hub.pair_device_with_offer(&offer.offer_id, &link_key(2), &platform, at(8, 1)).unwrap_err(),
        HubError::TextEmpty { .. }
    ));
    for err in [unknown, late, refused, known] {
        plain(&err.to_string());
    }
    unchanged(&hub);

    // At its last second, the same offer still pairs.
    let pairing = hub.pair_device_with_offer(&offer.offer_id, &link_key(2), &request("Second"), at(8, 10)).unwrap();
    assert_eq!(hub.link_phones().unwrap().len(), 2);
    assert_eq!(hub.offer(&offer.offer_id).unwrap().unwrap().device_id, Some(pairing.device_id));
}

#[test]
fn a_phones_name_is_made_fit_to_show_and_an_odd_version_is_not_kept() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let pair_as = |hub: &mut Hub, n: u8, name: &str, android_version: Option<u32>| {
        let offer = offer(hub, &asha, n, at(7, 0));
        let phone = PhoneRequest { android_version, ..request(name) };
        let id = hub.pair_device_with_offer(&offer.offer_id, &link_key(n), &phone, at(7, 1)).unwrap().device_id;
        hub.phone(&id).unwrap()
    };
    // The link allows 120 characters; the hub keeps 40, with nothing hidden.
    let long = pair_as(&mut hub, 1, &format!("Asha\u{202E}'s {}", "phone ".repeat(20)), Some(34));
    assert_eq!(long.name, "Asha's phone phone phone phone phone pho");
    assert_eq!(long.name.chars().count(), 40);
    assert_eq!(long.android_version, Some(34));
    let blank = pair_as(&mut hub, 2, "\u{200F}\u{0} ", Some(5_000));
    assert_eq!((blank.name.as_str(), blank.android_version), ("Phone", None));
    let none = pair_as(&mut hub, 3, "Tablet", None);
    assert_eq!(none.android_version, None);
}

#[test]
fn a_pairing_that_fails_part_way_leaves_nothing_behind() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let offer = offer(&mut hub, &asha, 1, at(7, 0));
    let before = (hub.family_today(at(7, 2)).unwrap(), hub.events(100, None).unwrap(), hub.book_revision(&asha).unwrap());
    hub.close().unwrap();

    // The last step fails (marking the offer used), and then the step before
    // it (keeping the phone's key): the book had taken the phone, its row
    // and its event were written.
    for trigger in [
        "CREATE TRIGGER fail BEFORE UPDATE ON offer BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
        "CREATE TRIGGER fail BEFORE INSERT ON phone BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
    ] {
        raw(&place.path).execute_batch(trigger).unwrap();
        let mut hub = Hub::open(&place.path).unwrap();
        let err = hub.pair_device_with_offer(&offer.offer_id, &link_key(1), &request("Phone"), at(7, 1)).unwrap_err();
        assert!(matches!(err, HubError::Storage { .. }), "{err:?}");
        assert_eq!((hub.family_today(at(7, 2)).unwrap(), hub.events(100, None).unwrap(), hub.book_revision(&asha).unwrap()), before);
        assert!(hub.link_phones().unwrap().is_empty());
        assert!(!hub.offer(&offer.offer_id).unwrap().unwrap().is_used());
        hub.close().unwrap();
        raw(&place.path).execute_batch("DROP TRIGGER fail;").unwrap();
        let conn = raw(&place.path);
        assert_eq!(count(&conn, "device", "1"), 0);
        assert_eq!(count(&conn, "phone", "1"), 0);
    }

    // With nothing in the way, the same offer pairs the same phone.
    let mut hub = Hub::open(&place.path).unwrap();
    hub.pair_device_with_offer(&offer.offer_id, &link_key(1), &request("Phone"), at(7, 1)).unwrap();
    assert_eq!(hub.link_phones().unwrap().len(), 1);
}

#[test]
fn a_removed_phone_stays_known_to_the_link_and_is_released_once() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut phone = pair_phone(&mut hub, &asha, "Old phone", 1, at(7, 0));
    let mut kept = pair_phone(&mut hub, &asha, "New phone", 2, at(7, 1));
    exchange(&mut hub, &mut phone, at(8, 0), false);
    hub.keep_phone_apps(&phone.id, &[limo_hub::PhoneApp { package: "com.a".into(), label: "A".into() }], "ab", at(8, 0))
        .unwrap();
    let err = hub.note_released(&phone.id, at(8, 1)).unwrap_err();
    assert_eq!(err, HubError::DeviceNotRemoved { device_id: phone.id.clone() });
    plain(&err.to_string());

    hub.remove_device(&phone.id, at(9, 0)).unwrap();
    // Still known to the link, as removed, so that it can be told so.
    let phones = hub.link_phones().unwrap();
    assert_eq!(phones.iter().map(|p| (p.link_key, p.removed)).collect::<Vec<_>>(), vec![
        (link_key(1), true),
        (link_key(2), false)
    ]);
    assert_eq!(hub.handle_report(&phone.state.report(at(9, 1), false), at(9, 1)).unwrap_err(), HubError::DeviceRemoved {
        device_id: phone.id.clone()
    });
    let removed = hub.phone(&phone.id).unwrap();
    assert_eq!((removed.removed_at, removed.released_at, removed.apps), (Some(at(9, 0)), None, None));
    assert_eq!(hub.phone_apps(&phone.id).unwrap(), None, "its apps go when it is removed");
    assert_eq!(hub.phones(&asha).unwrap().len(), 1);

    // Told, it lets go: noted once.
    assert!(hub.note_released(&phone.id, at(9, 5)).unwrap());
    assert!(!hub.note_released(&phone.id, at(9, 6)).unwrap());
    assert_eq!(hub.phone(&phone.id).unwrap().released_at, Some(at(9, 5)));
    let released: Vec<EventKind> = hub
        .events(100, None)
        .unwrap()
        .into_iter()
        .map(|e| e.what)
        .filter(|w| matches!(w, EventKind::DeviceReleased { .. }))
        .collect();
    assert_eq!(released, vec![EventKind::DeviceReleased { device_id: phone.id.clone(), name: "Old phone".into() }]);

    // Only phones are released.
    let pc = hub.pair_master_pc(&asha, "Family PC", at(9, 10)).unwrap();
    hub.remove_device(&pc.device_id, at(9, 11)).unwrap();
    let err = hub.note_released(&pc.device_id, at(9, 12)).unwrap_err();
    assert_eq!(err, HubError::NotAPhone { device_id: pc.device_id.clone() });
    plain(&err.to_string());
    assert!(matches!(hub.note_released("device-nobody", at(9, 12)).unwrap_err(), HubError::UnknownDevice { .. }));
    exchange(&mut hub, &mut kept, at(9, 13), false);
}

#[test]
fn offers_are_dropped_with_their_secrets_a_day_after_they_expire() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let (id, text) = offer_parts(7, "QUIXOTICSECRET");
    hub.keep_offer(&asha, &id, &text, at(7, 10), at(7, 0)).unwrap();
    let used = offer(&mut hub, &asha, 8, at(7, 0));
    hub.pair_device_with_offer(&used.offer_id, &link_key(8), &request("Phone"), at(7, 1)).unwrap();

    // Kept a day past its expiry, so that a phone late with it can be told.
    assert_eq!(hub.tidy(at(7, 10) + OFFER_KEEP_SECS).unwrap().offers_dropped, 0);
    assert!(hub.offer(&id).unwrap().is_some());
    assert_eq!(hub.tidy(at(7, 10) + OFFER_KEEP_SECS + 1).unwrap().offers_dropped, 2);
    assert_eq!((hub.offer(&id).unwrap(), hub.offer(&used.offer_id).unwrap()), (None, None));
    hub.close().unwrap();
    // Not only gone from the table: gone from the file.
    assert!(!file_holds(&place.path, "QUIXOTICSECRET"));
    assert!(!file_holds(&place.path, &text));
}
