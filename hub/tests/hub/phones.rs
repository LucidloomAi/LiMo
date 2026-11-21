//! What the master knows of each phone beside the child's book: the notes
//! its reports carry, kept once each, as events and as the newest of each
//! kind, on the disk with the grant that confirms them; and its apps, made
//! fit to show.

use limo_core::{DeviceKind, SentNote};
use limo_hub::{
    EventKind, GuardedPage, Hub, HubError, MAX_NOTES_PER_REPORT, PhoneApp, PhoneNote, PhonePermission, ProtectionLevel,
};

use crate::common::*;

use PhoneNote::{PermissionOff, PermissionOn};
use PhonePermission::{Accessibility, Battery};

fn noted(hub: &Hub) -> Vec<PhoneNote> {
    let mut notes: Vec<PhoneNote> = hub
        .events(500, None)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.what {
            EventKind::PhoneNoted { note, .. } => Some(note),
            _ => None,
        })
        .collect();
    notes.reverse();
    notes
}

/// One report from the phone carrying these notes, numbered from `first`;
/// the grant confirms every one of them.
fn report_with(hub: &mut Hub, phone: &mut Device, notes: &[PhoneNote], first: u64, now: i64) {
    let report = report_with_notes(phone, notes, first, now);
    let grant = hub.handle_report(&report, now).unwrap();
    let carried: Vec<String> = report.notes.iter().map(|n| n.id.clone()).collect();
    assert!(carried.iter().all(|id| grant.confirmed_notes.contains(id)));
    phone.state.take_grant(&grant, now);
}

fn app(package: &str, label: &str) -> PhoneApp {
    PhoneApp { package: package.into(), label: label.into() }
}

#[test]
fn notes_are_kept_once_as_events_and_as_the_newest_of_each_kind() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut phone = pair_phone(&mut hub, &asha, "Phone", 1, at(7, 0));

    // Setup on the phone: the six steps, then finished.
    let mut setup: Vec<PhoneNote> =
        PhonePermission::ALL.iter().enumerate().map(|(i, p)| PermissionOn { permission: *p, at: at(7, 1 + i as u8) }).collect();
    setup.push(PhoneNote::Protection { level: ProtectionLevel::Base, at: at(7, 7) });
    setup.push(PhoneNote::UsbDebugging { on: true, at: at(7, 7) });
    setup.push(PhoneNote::SetupFinished { at: at(7, 8) });
    report_with(&mut hub, &mut phone, &setup, 1, at(7, 8));
    let seen = hub.phone(&phone.id).unwrap();
    assert_eq!(seen.setup_finished_at, Some(at(7, 8)));
    assert_eq!(seen.switched_on, PhonePermission::ALL.to_vec());
    assert!(seen.switched_off.is_empty());
    assert_eq!((seen.protection, seen.usb_debugging_on), (Some(ProtectionLevel::Base), Some(true)));
    assert_eq!(noted(&hub), setup);

    // Reports carry each note until a grant confirms it: the same notes
    // again add nothing.
    let events = hub.events(500, None).unwrap();
    report_with(&mut hub, &mut phone, &setup, 1, at(7, 9));
    report_with(&mut hub, &mut phone, &[setup[2].clone()], 3, at(7, 10));
    assert_eq!(hub.events(500, None).unwrap(), events);
    assert_eq!(hub.phone(&phone.id).unwrap().notes, seen.notes);

    // Later, a permission switched off and on again, USB debugging switched
    // off, a guarded page, the guard opened, a restart and a clock moved.
    let later = [
        PermissionOff { permission: Accessibility, at: at(9, 0) },
        PermissionOff { permission: Battery, at: at(9, 1) },
        PhoneNote::GuardedPageOpened { page: GuardedPage::Accessibility, day: day_of(at(9, 2)), at: at(9, 2) },
        PhoneNote::GuardOpened { until: at(9, 8), at: at(9, 3) },
        PermissionOn { permission: Accessibility, at: at(9, 4) },
        PhoneNote::UsbDebugging { on: false, at: at(9, 5) },
        PhoneNote::Restarted { at: at(9, 6) },
        PhoneNote::ClockMoved { by_secs: -3_600, followed: true, at: at(9, 7) },
    ];
    report_with(&mut hub, &mut phone, &later, 10, at(9, 10));
    hub.close().unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    let seen = hub.phone(&phone.id).unwrap();
    assert_eq!(seen.switched_off, vec![Battery]);
    assert_eq!(seen.switched_on.len(), 5);
    assert_eq!(seen.usb_debugging_on, Some(false));
    assert_eq!(seen.notes, vec![
        PhoneNote::SetupFinished { at: at(7, 8) },
        PermissionOn { permission: PhonePermission::Notifications, at: at(7, 1) },
        PermissionOn { permission: PhonePermission::Phone, at: at(7, 2) },
        PermissionOn { permission: PhonePermission::UsageAccess, at: at(7, 3) },
        PermissionOn { permission: Accessibility, at: at(9, 4) },
        PermissionOff { permission: Battery, at: at(9, 1) },
        PermissionOn { permission: PhonePermission::Protection, at: at(7, 6) },
        PhoneNote::Protection { level: ProtectionLevel::Base, at: at(7, 7) },
        PhoneNote::UsbDebugging { on: false, at: at(9, 5) },
        later[2].clone(),
        later[3].clone(),
        later[6].clone(),
        later[7].clone(),
    ]);
    assert_eq!(noted(&hub), [setup, later.to_vec()].concat());
    let event = &hub.events(1, None).unwrap()[0];
    assert_eq!((event.at, event.child_id.as_deref()), (at(9, 10), Some(asha.as_str())));

    // The newest note wins by the order notes came in, not by their times:
    // a phone's clock can be set back.
    report_with(&mut hub, &mut phone, &[PermissionOn { permission: Battery, at: at(8, 0) }], 18, at(9, 20));
    assert!(hub.phone(&phone.id).unwrap().switched_off.is_empty());
    // The phones of a child are listed with what their notes say.
    assert_eq!(hub.phones(&asha).unwrap(), vec![hub.phone(&phone.id).unwrap()]);
}

#[test]
fn at_most_the_newest_notes_of_a_report_are_taken() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut phone = pair_phone(&mut hub, &asha, "Phone", 1, at(7, 0));
    let flood: Vec<PhoneNote> = (0..250).map(|i| PhoneNote::Restarted { at: at(8, 0) + i }).collect();
    report_with(&mut hub, &mut phone, &flood, 1, at(8, 5));
    let noted = noted(&hub);
    assert_eq!(noted.len(), MAX_NOTES_PER_REPORT);
    assert_eq!(noted.first(), Some(&PhoneNote::Restarted { at: at(8, 0) + 50 }));
    assert_eq!(hub.phone(&phone.id).unwrap().notes, vec![PhoneNote::Restarted { at: at(8, 0) + 249 }]);
}

#[test]
fn notes_are_on_the_disk_with_the_grant_that_confirms_them() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut phone = pair_phone(&mut hub, &asha, "Phone", 1, at(7, 0));
    phone.state.record_use("com.google.android.youtube", at(8, 0), at(8, 10));
    let report = report_with_notes(&mut phone, &[PhoneNote::SetupFinished { at: at(8, 1) }], 1, at(8, 10));
    let before = (hub.family_today(at(8, 10)).unwrap(), hub.events(100, None).unwrap(), hub.book_revision(&asha).unwrap());
    hub.close().unwrap();

    // Keeping the notes fails: there is no grant, and the report's use is
    // not kept either, so the phone keeps both and sends them again.
    raw(&place.path)
        .execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON phone BEGIN SELECT RAISE(ABORT, 'test failure'); END;")
        .unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    let err = hub.handle_report(&report, at(8, 10)).unwrap_err();
    assert!(matches!(err, HubError::Storage { .. }), "{err:?}");
    assert_eq!((hub.family_today(at(8, 10)).unwrap(), hub.events(100, None).unwrap(), hub.book_revision(&asha).unwrap()), before);
    assert_eq!(hub.phone(&phone.id).unwrap().setup_finished_at, None);
    hub.close().unwrap();
    raw(&place.path).execute_batch("DROP TRIGGER fail;").unwrap();

    let mut hub = Hub::open(&place.path).unwrap();
    let grant = hub.handle_report(&report, at(8, 11)).unwrap();
    assert_eq!(grant.confirmed_notes, vec![SentNote::id_for(&phone.id, 1)]);
    phone.state.take_grant(&grant, at(8, 11));
    assert_eq!(hub.child_today(&asha, at(8, 11)).unwrap().used_secs, 10 * MIN as u64);
    assert_eq!(hub.phone(&phone.id).unwrap().setup_finished_at, Some(at(8, 1)));
}

#[test]
fn notes_come_only_from_phones_and_a_phone_paired_before_the_link_takes_them_too() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut pc = hub.pair_master_pc(&asha, "Family PC", at(7, 0)).unwrap();
    let mut old = pair(&mut hub, &asha, "Old phone", DeviceKind::Portable, at(7, 0));
    let note = PhoneNote::Restarted { at: at(8, 0) };

    // The master PC's own device is no phone: refused, and nothing is kept,
    // not even the report.
    let revision = hub.book_revision(&asha).unwrap();
    let mut report = pc.state.report(at(8, 0), false);
    report.notes = vec![SentNote { id: SentNote::id_for(&pc.device_id, 1), note: note.clone() }];
    let err = hub.handle_report(&report, at(8, 0)).unwrap_err();
    assert_eq!(err, HubError::NotAPhone { device_id: pc.device_id.clone() });
    plain(&err.to_string());
    assert_eq!(hub.book_revision(&asha).unwrap(), revision);
    assert!(matches!(hub.phone(&pc.device_id).unwrap_err(), HubError::NotAPhone { .. }));
    hub.exchange_on_master_pc(&mut pc.state, false, "", at(8, 1)).unwrap();
    // A report with no notes is just a report.
    let report = old.state.report(at(8, 2), false);
    hub.handle_report(&report, at(8, 2)).unwrap();

    // A phone paired the old way has no link key, and keeps its notes.
    report_with(&mut hub, &mut old, std::slice::from_ref(&note), 1, at(8, 3));
    let seen = hub.phone(&old.id).unwrap();
    assert_eq!((seen.has_link_key, seen.notes.clone()), (false, vec![note.clone()]));
    assert!(hub.link_phones().unwrap().is_empty());
    // A removed phone's report is refused, notes and all.
    hub.remove_device(&old.id, at(9, 0)).unwrap();
    let report = report_with_notes(&mut old, &[PhoneNote::Restarted { at: at(9, 1) }], 2, at(9, 1));
    let err = hub.handle_report(&report, at(9, 1)).unwrap_err();
    assert_eq!(err, HubError::DeviceRemoved { device_id: old.id.clone() });
    assert_eq!(hub.phone(&old.id).unwrap().notes, vec![note]);
}

#[test]
fn a_phones_apps_are_kept_once_and_made_fit_to_show() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let phone = pair_phone(&mut hub, &asha, "Phone", 1, at(7, 0));
    assert_eq!(hub.phone_apps(&phone.id).unwrap(), None);

    let sent = vec![
        app("com.google.android.youtube", "YouTube"),
        app("com.whatsapp", "WhatsApp\u{202E}gnp.exe"),
        app("com.google.android.youtube", "YouTube again"),
        app("com/../evil", "Evil"),
        app("", "Nothing"),
        app("com.example.blank", " \u{200F} "),
        app("com.example.long", &"x".repeat(200)),
        app("  com.example.spaced  ", "Spaced"),
    ];
    assert!(hub.keep_phone_apps(&phone.id, &sent, "0123456789ABCDEF0123456789abcdef", at(7, 30)).unwrap());
    let kept = hub.phone_apps(&phone.id).unwrap().unwrap();
    assert_eq!(kept.apps, vec![
        app("com.google.android.youtube", "YouTube"),
        app("com.whatsapp", "WhatsAppgnp.exe"),
        app("com.example.blank", "com.example.blank"),
        app("com.example.long", &"x".repeat(120)),
        app("com.example.spaced", "Spaced"),
    ]);
    assert_eq!((kept.hash.as_str(), kept.kept_at), ("0123456789abcdef0123456789abcdef", at(7, 30)));
    assert_eq!(hub.phone(&phone.id).unwrap().apps, Some(5));

    // The same list again changes nothing; a new list replaces it.
    assert!(!hub.keep_phone_apps(&phone.id, &sent, "0123456789abcdef0123456789abcdef", at(7, 40)).unwrap());
    assert_eq!(hub.phone_apps(&phone.id).unwrap().unwrap().kept_at, at(7, 30));
    assert!(hub.keep_phone_apps(&phone.id, &[app("com.whatsapp", "WhatsApp")], "ff", at(7, 50)).unwrap());
    assert_eq!(hub.phone_apps(&phone.id).unwrap().unwrap().apps, vec![app("com.whatsapp", "WhatsApp")]);
    hub.close().unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.phone_apps(&phone.id).unwrap().unwrap().hash, "ff");

    // Too many apps, or a hash LiMo does not make, is refused, and nothing
    // changes.
    let many: Vec<PhoneApp> = (0..2_001).map(|i| app(&format!("com.example.app{i}"), "App")).collect();
    for (apps, hash) in [(&many[..], "ff"), (&many[..2_000], ""), (&many[..1], "not hex"), (&many[..1], &"a".repeat(65))] {
        let err = hub.keep_phone_apps(&phone.id, apps, hash, at(8, 0)).unwrap_err();
        assert_eq!(err, HubError::BadAppList { most_apps: 2_000 });
        plain(&err.to_string());
    }
    assert_eq!(hub.phone_apps(&phone.id).unwrap().unwrap().apps.len(), 1);
    assert!(hub.keep_phone_apps(&phone.id, &many[..2_000], "ab", at(8, 0)).unwrap());
    assert_eq!(hub.phone(&phone.id).unwrap().apps, Some(2_000));

    // Only a paired phone has apps.
    let pc = hub.pair_master_pc(&asha, "Family PC", at(8, 1)).unwrap();
    assert!(matches!(hub.keep_phone_apps(&pc.device_id, &[], "ab", at(8, 2)).unwrap_err(), HubError::NotAPhone { .. }));
    assert!(matches!(hub.phone_apps(&pc.device_id).unwrap_err(), HubError::NotAPhone { .. }));
    assert!(matches!(hub.phone_apps("device-nobody").unwrap_err(), HubError::UnknownDevice { .. }));
    hub.remove_device(&phone.id, at(8, 3)).unwrap();
    let err = hub.keep_phone_apps(&phone.id, &[], "ab", at(8, 4)).unwrap_err();
    assert_eq!(err, HubError::DeviceRemoved { device_id: phone.id.clone() });
}

#[test]
fn what_a_phone_says_it_is_is_kept_made_fit_and_only_for_a_paired_phone() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let phone = pair_phone(&mut hub, &asha, "Asha\u{2019}s phone", 1, at(8, 0));
    let pc = pair(&mut hub, &asha, "Family PC", DeviceKind::OnMaster, at(8, 0));
    // Paired: Android's level from the pairing, and nothing said yet.
    let paired = hub.phone(&phone.id).unwrap();
    assert_eq!((paired.android_version, paired.model, paired.app_version), (Some(33), None, None));
    // Said: all three kept, replacing the pairing's level.
    assert!(hub.keep_phone_about(&phone.id, "samsung SM-S911B", Some(35), "0.4.0").unwrap());
    let said = hub.phone(&phone.id).unwrap();
    assert_eq!(
        (said.android_version, said.model.as_deref(), said.app_version.as_deref()),
        (Some(35), Some("samsung SM-S911B"), Some("0.4.0"))
    );
    // The same again changes nothing.
    assert!(!hub.keep_phone_about(&phone.id, "samsung SM-S911B", Some(35), "0.4.0").unwrap());
    // Made fit: hidden characters out, cut to the limits; empty is none.
    let long = format!("  \u{202e}{}\u{0007} ", "M".repeat(limo_hub::MAX_PHONE_MODEL_CHARS + 20));
    assert!(hub.keep_phone_about(&phone.id, &long, Some(35), &"9".repeat(60)).unwrap());
    let fit = hub.phone(&phone.id).unwrap();
    assert_eq!(fit.model.as_deref().map(|m| m.chars().count()), Some(limo_hub::MAX_PHONE_MODEL_CHARS));
    assert!(fit.model.as_deref().unwrap().chars().all(|c| c == 'M'));
    assert_eq!(fit.app_version.as_deref().map(str::len), Some(limo_hub::MAX_APP_VERSION_CHARS));
    assert!(hub.keep_phone_about(&phone.id, "   ", None, "").unwrap());
    let empty = hub.phone(&phone.id).unwrap();
    assert_eq!((empty.android_version, empty.model, empty.app_version), (None, None, None));
    assert!(hub.keep_phone_about(&phone.id, "samsung SM-S911B", Some(35), "0.4.0").unwrap());
    // Only for a phone, and only while it is paired.
    assert!(matches!(hub.keep_phone_about(&pc.id, "x", None, "1"), Err(HubError::NotAPhone { .. })));
    hub.remove_device(&phone.id, at(9, 0)).unwrap();
    assert!(matches!(hub.keep_phone_about(&phone.id, "x", None, "1"), Err(HubError::DeviceRemoved { .. })));
    // It is no event, and kept across a restart.
    assert!(hub.events(500, None).unwrap().iter().all(|e| !format!("{:?}", e.what).contains("SM-S911B")));
    hub.close().unwrap();
    let hub = Hub::open(&place.path).unwrap();
    let kept = hub.phone(&phone.id).unwrap();
    assert_eq!(
        (kept.model.as_deref(), kept.app_version.as_deref(), kept.android_version),
        (Some("samsung SM-S911B"), Some("0.4.0"), Some(35)),
        "the last said, after the restart"
    );
}

#[test]
fn codes_left_today_count_down_from_ten_and_start_again_the_next_day() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let pc = pair(&mut hub, &asha, "Family PC", DeviceKind::OnMaster, at(8, 0));
    let phone = pair_phone(&mut hub, &asha, "Asha\u{2019}s phone", 2, at(8, 0));
    assert_eq!(hub.codes_left(&pc.id, at(9, 0)).unwrap(), limo_core::CODES_PER_DAY);
    hub.make_code(&pc.id, 15, at(9, 0)).unwrap();
    hub.make_code(&pc.id, 60, at(9, 1)).unwrap();
    assert_eq!(hub.codes_left(&pc.id, at(9, 2)).unwrap(), 8, "codes of any minutes count");
    assert_eq!(hub.codes_left(&phone.id, at(9, 2)).unwrap(), 10, "each device its own");
    for minute in 0..8 {
        hub.make_code(&pc.id, 30, at(10, minute)).unwrap();
    }
    assert_eq!(hub.codes_left(&pc.id, at(11, 0)).unwrap(), 0);
    assert!(hub.make_code(&pc.id, 15, at(11, 0)).is_err(), "and none is made then");
    // The next home day, ten again.
    assert_eq!(hub.codes_left(&pc.id, at(11, 0) + 24 * HOUR).unwrap(), 10);
    // A removed device has none to give.
    hub.remove_device(&phone.id, at(12, 0)).unwrap();
    assert!(matches!(hub.codes_left(&phone.id, at(12, 0)), Err(HubError::DeviceRemoved { .. })));
}
