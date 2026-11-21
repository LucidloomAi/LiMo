//! Files made by earlier versions of the hub open, are carried to this
//! version, and go on: one of the first version, one from before the agent,
//! one from before the link, and one from before phones said what they are.
//!
//! A file made by the first version of the hub (schema version 1, with the
//! rules core of that time) opens, is carried to this version, and goes on.
//!
//! `tests/fixtures/store_v1.limo` was made with the code at the commit "Add
//! the hub store", before any of this change: a family with the password of
//! the stories and the PIN 482915; Asha with the family PC, a phone and an
//! old tablet, and Ravi with a tablet; use on Monday 9 November (folded into
//! daily totals by `tidy` with a week in detail), Tuesday 17 and Wednesday
//! 18; extra time given, a request granted, two codes made, one of them used
//! in part on the old tablet, which was then removed (the first hub kept it
//! in the book as a retired record); two Windows accounts; a lock on Ravi.
//! The `store_v1_*.json` files beside it are the device states of that
//! moment, a report the phone had already sent, and what that code's views
//! showed at Wednesday 12:00.

use limo_core::{DeviceKind, DeviceState, ExtraTime, Report, Slice};
use limo_hub::{ChildToday, DeviceInfo, Event, Hub, HubError, SCHEMA_VERSION, UsageView};
use rusqlite::Connection;

use crate::common::*;

/// Wednesday 18 November 2026 at 12:00, when the file was saved.
const NOW: i64 = 1_794_983_400;
const TODAY: i64 = 20_775;

fn json<T: serde::de::DeserializeOwned>(text: &str) -> T {
    serde_json::from_str(text).unwrap()
}

fn version(conn: &Connection) -> i64 {
    conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap()
}

fn books(conn: &Connection) -> Vec<String> {
    let mut stmt = conn.prepare("SELECT json FROM book ORDER BY child_id").unwrap();
    stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<Vec<String>, _>>().unwrap()
}

#[test]
fn a_file_made_by_the_first_hub_opens_converts_and_goes_on() {
    let place = place();
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/store_v1.limo"), &place.path).unwrap();
    let devices: (Vec<DeviceInfo>, Vec<DeviceInfo>) = json(include_str!("../fixtures/store_v1_devices.json"));
    let asha = devices.0[0].child_id.clone();
    let ravi = devices.1[0].child_id.clone();
    let (pc_id, phone_id, tab_id) = (devices.0[0].device_id.clone(), devices.0[1].device_id.clone(), devices.1[0].device_id.clone());
    let mut pc = Device { id: pc_id, state: json(include_str!("../fixtures/store_v1_pc_state.json")) };
    let mut phone = Device { id: phone_id, state: json(include_str!("../fixtures/store_v1_phone_state.json")) };
    let mut tab = Device { id: tab_id, state: json(include_str!("../fixtures/store_v1_ravi_tablet_state.json")) };
    let mut old: DeviceState = json(include_str!("../fixtures/store_v1_old_tablet_state.json"));
    let old_id = old.device_id();

    // As the first hub left it: version 1, books with whole slices, and the
    // old tablet still in Asha's book as a retired record.
    {
        let conn = raw(&place.path);
        assert_eq!(version(&conn), 1);
        let saved = books(&conn);
        assert!(saved.iter().any(|b| b.contains("\"slices\":{\"")));
        assert!(saved.iter().any(|b| b.contains(&format!("\"device_id\":\"{old_id}\",\"kind\":\"on_master\""))));
    }

    let mut hub = Hub::open(&place.path).unwrap();
    // What the guardian sees is what the first hub showed.
    let today: Vec<ChildToday> = json(include_str!("../fixtures/store_v1_family_today.json"));
    assert_eq!(hub.family_today(NOW).unwrap(), today);
    let week: UsageView = json(include_str!("../fixtures/store_v1_usage_week.json"));
    assert_eq!(hub.usage(&asha, TODAY - 9, TODAY).unwrap(), week);
    let day: UsageView = json(include_str!("../fixtures/store_v1_usage_today.json"));
    assert_eq!(hub.usage(&asha, TODAY, TODAY).unwrap(), day);
    let extra: Vec<ExtraTime> = json(include_str!("../fixtures/store_v1_extra_today.json"));
    assert_eq!(hub.extra_time_today(&asha, NOW).unwrap(), extra);
    let events: Vec<Event> = json(include_str!("../fixtures/store_v1_events.json"));
    assert_eq!(hub.events(100, None).unwrap(), events);
    assert_eq!((hub.devices(&asha).unwrap(), hub.devices(&ravi).unwrap()), devices);
    assert!(hub.wanted_reports(NOW).unwrap().is_empty());
    assert!(hub.check_password(PASSWORD, NOW).is_ok() && hub.has_pin().unwrap());
    hub.close().unwrap();

    // The file is at this version, and its books are in the core's present
    // shape: no slices, and the old tablet removed by the core itself.
    {
        let conn = raw(&place.path);
        assert_eq!(version(&conn), SCHEMA_VERSION as i64);
        let saved = books(&conn);
        assert!(saved.iter().all(|b| !b.contains("\"slices\"")));
        let book: serde_json::Value = json(saved.iter().find(|b| b.contains(&old_id)).unwrap());
        assert!(book["devices"].as_array().unwrap().iter().all(|d| d["device_id"] != old_id.as_str()));
        assert!(book["removed"].as_object().unwrap().contains_key(&old_id));
    }

    // Everything goes on. The PC reports the two minutes it had not yet
    // reported; they count once, and are kept once.
    let mut hub = Hub::open(&place.path).unwrap();
    let before = hub.child_today(&asha, NOW).unwrap();
    exchange(&mut hub, &mut pc, NOW, false);
    exchange(&mut hub, &mut pc, NOW + 60, false);
    let after = hub.child_today(&asha, NOW + 60).unwrap();
    assert_eq!(after.used_secs, before.used_secs + 2 * MIN as u64);
    assert_eq!(hub.usage(&asha, TODAY, TODAY).unwrap().secs, day.secs + 2 * MIN as u64);
    // A report the phone sent before the upgrade, sent again, adds nothing;
    // and every slice the old book had taken, sent again, adds nothing.
    let repeated: Report = json(include_str!("../fixtures/store_v1_repeated_report.json"));
    assert!(!repeated.slices.is_empty());
    hub.handle_report(&repeated, NOW + 61).unwrap();
    let mut again = phone.state.report(NOW + 62, false);
    let slices: Vec<Slice> = {
        hub.close().unwrap();
        let conn = raw(&place.path);
        let mut stmt = conn
            .prepare("SELECT id, device_id, app_id, day, start_at, end_at, counts FROM slice WHERE device_id = ?1")
            .unwrap();
        let rows = stmt
            .query_map([&phone.id], |r| {
                Ok(Slice {
                    id: r.get(0)?,
                    child_id: asha.clone(),
                    device_id: r.get(1)?,
                    app_id: r.get(2)?,
                    day: r.get(3)?,
                    start: r.get(4)?,
                    end: r.get(5)?,
                    counts: r.get(6)?,
                })
            })
            .unwrap();
        rows.collect::<Result<Vec<_>, _>>().unwrap()
    };
    let mut hub = Hub::open(&place.path).unwrap();
    assert!(slices.len() > 5);
    again.slices = slices;
    let grant = hub.handle_report(&again, NOW + 62).unwrap();
    phone.state.take_grant(&grant, NOW + 62);
    assert_eq!(hub.child_today(&asha, NOW + 62).unwrap().used_secs, after.used_secs);
    assert_eq!(hub.usage(&asha, TODAY, TODAY).unwrap().secs, day.secs + 2 * MIN as u64);
    // The phone is used, and its use counts.
    use_app(&mut hub, &mut phone, "video", NOW + 120, NOW + 420);
    assert_eq!(hub.child_today(&asha, NOW + 420).unwrap().used_secs, after.used_secs + 5 * MIN as u64);
    // The old tablet stays refused; the code time added on it stays in the
    // day's extra, since the retired record no longer said how much of it
    // was used.
    let err = hub.handle_report(&old.report(NOW + 430, false), NOW + 430).unwrap_err();
    assert_eq!(err, HubError::DeviceRemoved { device_id: old_id.clone() });
    assert_eq!(hub.child_today(&asha, NOW + 430).unwrap().extra_secs, before.extra_secs);
    // Ravi's tablet goes on too, and a device can be paired and removed.
    exchange(&mut hub, &mut tab, NOW + 440, false);
    let mut new_phone = pair(&mut hub, &asha, "New phone", DeviceKind::Portable, NOW + 450);
    exchange(&mut hub, &mut new_phone, NOW + 451, false);
    hub.remove_device(&new_phone.id, NOW + 452).unwrap();
    hub.tidy(NOW + 500).unwrap();
    assert_eq!(hub.child_today(&asha, NOW + 500).unwrap().devices.len(), 2);
}

/// `tests/fixtures/store_v2.limo` was made with the hub as committed at
/// "Rules core and hub: what the first real caller needed" (schema version
/// 2), before the agent on the master PC: a family with the password of the
/// stories and the PIN 482915; Asha with a PC device paired the way that hub
/// could (no state kept in the file), a phone, use on both, and extra time;
/// Ravi with a tablet, use, and a lock until 20:00; three Windows accounts.
/// The `store_v2_*.json` files beside it are the device states of that
/// moment and what that code's views showed at Wednesday 12:00.
#[test]
fn a_file_made_by_the_hub_before_the_agent_opens_and_goes_on() {
    let place = place();
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/store_v2.limo"), &place.path).unwrap();
    let devices: (Vec<DeviceInfo>, Vec<DeviceInfo>) = json(include_str!("../fixtures/store_v2_devices.json"));
    let asha = devices.0[0].child_id.clone();
    let ravi = devices.1[0].child_id.clone();
    let old_pc_id = devices.0[0].device_id.clone();
    let mut phone = Device { id: devices.0[1].device_id.clone(), state: json(include_str!("../fixtures/store_v2_phone_state.json")) };
    let mut tablet = Device { id: devices.1[0].device_id.clone(), state: json(include_str!("../fixtures/store_v2_tablet_state.json")) };
    let old_pc: DeviceState = json(include_str!("../fixtures/store_v2_pc_state.json"));
    assert_eq!(version(&raw(&place.path)), 2);

    let hub = Hub::open(&place.path).unwrap();
    // What the guardian sees is what the hub before the agent showed.
    let today: Vec<ChildToday> = json(include_str!("../fixtures/store_v2_family_today.json"));
    assert_eq!(hub.family_today(NOW).unwrap(), today);
    let day: UsageView = json(include_str!("../fixtures/store_v2_usage_today.json"));
    assert_eq!(hub.usage(&asha, TODAY, TODAY).unwrap(), day);
    let events: Vec<Event> = json(include_str!("../fixtures/store_v2_events.json"));
    assert_eq!(hub.events(100, None).unwrap(), events);
    assert_eq!((hub.devices(&asha).unwrap(), hub.devices(&ravi).unwrap()), devices);
    assert_eq!(hub.pc_settings().unwrap(), limo_hub::PcSettings {
        idle_minutes: limo_hub::DEFAULT_IDLE_MINUTES,
        unlinked_accounts: limo_hub::UnlinkedAccounts::Free
    });
    assert_eq!(hub.book_revision(&asha).unwrap(), 0);
    hub.close().unwrap();
    assert_eq!(version(&raw(&place.path)), SCHEMA_VERSION as i64);

    // The phone and the tablet go on.
    let mut hub = Hub::open(&place.path).unwrap();
    let before = hub.child_today(&asha, NOW).unwrap();
    use_app(&mut hub, &mut phone, "com.google.android.youtube", NOW, NOW + 300);
    assert_eq!(hub.child_today(&asha, NOW + 300).unwrap().used_secs, before.used_secs + 5 * MIN as u64);
    assert!(hub.book_revision(&asha).unwrap() > 0);
    exchange(&mut hub, &mut tablet, NOW + 310, false);

    // The PC device paired before the agent has no state kept, so the agent
    // has it replaced, once; its use stays counted.
    assert_eq!(hub.master_pc(&asha).unwrap(), None);
    let pc = hub.pair_master_pc(&asha, "Family PC", NOW + 320).unwrap();
    assert_ne!(pc.device_id, old_pc_id);
    assert_eq!(hub.child_today(&asha, NOW + 320).unwrap().used_secs, before.used_secs + 5 * MIN as u64);
    let err = hub.handle_report(&old_pc.clone().report(NOW + 321, false), NOW + 321).unwrap_err();
    assert_eq!(err, HubError::DeviceRemoved { device_id: old_pc_id });
    assert_eq!(hub.pair_master_pc(&asha, "Family PC", NOW + 322).unwrap(), pc);
    let mut state = pc.state.clone();
    state.record_use("minecraft.exe", NOW + 322, NOW + 382);
    hub.exchange_on_master_pc(&mut state, true, "", NOW + 382).unwrap();
    assert_eq!(hub.child_today(&asha, NOW + 382).unwrap().used_secs, before.used_secs + 6 * MIN as u64);
    // Ravi gets one of his own, and his lock reaches it.
    let mut ravi_pc = hub.pair_master_pc(&ravi, "Family PC", NOW + 400).unwrap().state;
    hub.exchange_on_master_pc(&mut ravi_pc, true, "", NOW + 401).unwrap();
    let decision = ravi_pc.decide(NOW + 402, None);
    assert_eq!(decision.device_lock.map(|l| l.reason), Some(limo_core::LockReason::GuardianLock));
    hub.tidy(NOW + 500).unwrap();
}

fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    conn.query_row("SELECT count(*) FROM pragma_table_info(?1) WHERE name = ?2", [table, column], |r| r.get::<_, i64>(0))
        .unwrap()
        == 1
}

/// `tests/fixtures/store_v3.limo` was made with the hub as it was before the
/// link kept anything in it (schema version 3), by a program that called
/// only that hub's public calls: a family with the password of the stories
/// and the PIN 482915; Asha with her device on the master PC (paired by the
/// agent, its state and notes kept), a phone paired the only way that hub
/// could (no link key), use on Tuesday 17 and Wednesday 18 November, a
/// request for 20 minutes waiting, extra time given and a code made, and an
/// old phone removed; Ravi with a tablet, his device on the master PC and a
/// lock until 20:00; three Windows accounts; the PC's settings changed; and
/// the agent's events. The `store_v3_*.json` files beside it are the device
/// states of that moment and what that hub's views showed at Wednesday
/// 12:00.
#[test]
fn a_file_made_by_the_hub_before_the_link_opens_and_goes_on() {
    use limo_core::RequestReply;
    use limo_hub::{MasterPcRecord, PcSettings, PhoneForm, PhoneNote, UnlinkedAccounts, WaitingRequest};

    let place = place();
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/store_v3.limo"), &place.path).unwrap();
    let devices: (Vec<DeviceInfo>, Vec<DeviceInfo>) = json(include_str!("../fixtures/store_v3_devices.json"));
    let asha = devices.0[0].child_id.clone();
    let ravi = devices.1[0].child_id.clone();
    let mut phone = Device { id: devices.0[1].device_id.clone(), state: json(include_str!("../fixtures/store_v3_phone_state.json")) };
    let mut tablet = Device { id: devices.1[0].device_id.clone(), state: json(include_str!("../fixtures/store_v3_tablet_state.json")) };
    let old_phone: DeviceState = json(include_str!("../fixtures/store_v3_old_phone_state.json"));
    let pcs: (Option<MasterPcRecord>, Option<MasterPcRecord>) = json(include_str!("../fixtures/store_v3_master_pc.json"));
    let waiting: Vec<WaitingRequest> = json(include_str!("../fixtures/store_v3_waiting.json"));
    {
        let conn = raw(&place.path);
        assert_eq!(version(&conn), 3);
        assert!(!has_column(&conn, "family", "master_key"));
    }

    let hub = Hub::open(&place.path).unwrap();
    // What the guardian sees is what the hub before the link showed.
    let today: Vec<ChildToday> = json(include_str!("../fixtures/store_v3_family_today.json"));
    assert_eq!(hub.family_today(NOW).unwrap(), today);
    let usage: UsageView = json(include_str!("../fixtures/store_v3_usage.json"));
    assert_eq!(hub.usage(&asha, TODAY - 1, TODAY).unwrap(), usage);
    let events: Vec<Event> = json(include_str!("../fixtures/store_v3_events.json"));
    assert_eq!(hub.events(100, None).unwrap(), events);
    assert_eq!((hub.devices(&asha).unwrap(), hub.devices(&ravi).unwrap()), devices);
    assert_eq!((hub.master_pc(&asha).unwrap(), hub.master_pc(&ravi).unwrap()), pcs);
    assert_eq!(hub.waiting_requests(NOW).unwrap(), waiting);
    assert_eq!(hub.pc_settings().unwrap(), PcSettings { idle_minutes: 5, unlinked_accounts: UnlinkedAccounts::Locked });
    assert_eq!(hub.accounts().unwrap().len(), 3);

    // The family got its master key, and every phone its row: the form of a
    // child's phone, no link key, no notes, its removal kept.
    let key = hub.master_key().unwrap();
    assert_ne!(key.expose_private(), &[0u8; 32]);
    assert!(hub.link_phones().unwrap().is_empty());
    let seen = hub.phone(&phone.id).unwrap();
    assert_eq!((seen.form, seen.has_link_key, seen.android_version), (PhoneForm::Child, false, None));
    assert!(seen.notes.is_empty() && seen.setup_finished_at.is_none() && seen.apps.is_none());
    assert_eq!(hub.phone(&tablet.id).unwrap().child_id, ravi);
    assert!(hub.phone(&old_phone.device_id()).unwrap().removed_at.is_some());
    let pc_id = pcs.0.as_ref().unwrap().device_id.clone();
    assert!(matches!(hub.phone(&pc_id).unwrap_err(), HubError::NotAPhone { .. }));
    hub.close().unwrap();
    {
        let conn = raw(&place.path);
        assert_eq!(version(&conn), SCHEMA_VERSION as i64);
        assert_eq!(count(&conn, "phone", "1"), 3);
    }

    // The key is made once: the same after a restart.
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.master_key().unwrap(), key);
    assert!(hub.check_password(PASSWORD, NOW).is_ok() && hub.has_pin().unwrap());

    // Everything goes on. The phone and the tablet report; the request is
    // answered and reaches the phone; the PC's device exchanges from its
    // kept state; the old phone stays refused.
    let before = hub.child_today(&asha, NOW).unwrap();
    use_app(&mut hub, &mut phone, "com.google.android.youtube", NOW, NOW + 300);
    exchange(&mut hub, &mut tablet, NOW + 310, false);
    hub.answer_request(&asha, &waiting[0].request.request.id, RequestReply::Grant { minutes: 20 }, NOW + 320).unwrap();
    exchange(&mut hub, &mut phone, NOW + 330, false);
    assert_eq!(hub.child_today(&asha, NOW + 330).unwrap().extra_secs, before.extra_secs + 20 * MIN as u64);
    let mut pc = pcs.0.unwrap().state;
    pc.record_use("minecraft.exe", NOW + 300, NOW + 360);
    hub.exchange_on_master_pc(&mut pc, true, "", NOW + 360).unwrap();
    assert_eq!(hub.child_today(&asha, NOW + 360).unwrap().used_secs, before.used_secs + 6 * MIN as u64);
    let mut old_phone = old_phone;
    let err = hub.handle_report(&old_phone.report(NOW + 370, false), NOW + 370).unwrap_err();
    assert_eq!(err, HubError::DeviceRemoved { device_id: old_phone.device_id() });

    // The phone paired before the link takes notes; a new phone pairs with
    // an offer, and the link knows its key.
    let report = report_with_notes(&mut phone, &[PhoneNote::SetupFinished { at: NOW }], 1, NOW + 380);
    hub.handle_report(&report, NOW + 380).unwrap();
    assert_eq!(hub.phone(&phone.id).unwrap().setup_finished_at, Some(NOW));
    let mut new_phone = pair_phone(&mut hub, &asha, "Asha's new phone", 9, NOW + 400);
    use_app(&mut hub, &mut new_phone, "com.duolingo", NOW + 400, NOW + 460);
    assert_eq!(hub.link_phones().unwrap().len(), 1);
    assert_eq!(hub.child_today(&asha, NOW + 460).unwrap().devices.len(), 3);
    hub.tidy(NOW + 500).unwrap();
}

/// `tests/fixtures/store_v4.limo` was made by the hub of version 4, before
/// a phone could say what it is, by a program that called only that hub's
/// public calls: a family with the password of the stories, Asha with the
/// family PC and a phone paired with an offer (Android's level 33 at
/// pairing), the phone's notes (setup finished, accessibility off, USB
/// debugging on), its list of one app, and two codes made for the PC. The
/// `store_v4_*.json` files beside it are what that hub showed at Wednesday
/// 12:00, and the two devices' states.
#[test]
fn a_file_made_by_the_hub_before_phones_said_what_they_are_opens_and_goes_on() {
    use limo_hub::PhoneInfo;

    let place = place();
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/store_v4.limo"), &place.path).unwrap();
    let devices: Vec<DeviceInfo> = json(include_str!("../fixtures/store_v4_devices.json"));
    let asha = devices[0].child_id.clone();
    let shown: PhoneInfo = json(include_str!("../fixtures/store_v4_phone.json"));
    let mut phone = Device { id: shown.device_id.clone(), state: json(include_str!("../fixtures/store_v4_phone_state.json")) };
    let mut pc = Device { id: devices[0].device_id.clone(), state: json(include_str!("../fixtures/store_v4_pc_state.json")) };
    {
        let conn = raw(&place.path);
        assert_eq!(version(&conn), 4);
        assert!(!has_column(&conn, "phone", "model"));
    }

    let hub = Hub::open(&place.path).unwrap();
    // What the guardian sees is what the hub before showed; the phone has
    // said nothing of what it is yet, and keeps its level from the pairing.
    let today: Vec<ChildToday> = json(include_str!("../fixtures/store_v4_family_today.json"));
    assert_eq!(hub.family_today(NOW).unwrap(), today);
    assert_eq!(hub.devices(&asha).unwrap(), devices);
    let seen = hub.phone(&phone.id).unwrap();
    assert_eq!(seen, shown, "the same, with no model and no version of LiMo");
    assert_eq!((seen.android_version, seen.model.as_deref(), seen.app_version.as_deref()), (Some(33), None, None));
    assert!(seen.setup_finished_at.is_some() && seen.usb_debugging_on == Some(true) && seen.apps == Some(1));
    // The codes made that day count, in a file older than the call.
    assert_eq!(hub.codes_left(&pc.id, NOW).unwrap(), limo_core::CODES_PER_DAY - 2);
    assert_eq!(hub.codes_left(&phone.id, NOW).unwrap(), limo_core::CODES_PER_DAY);
    hub.close().unwrap();
    {
        let conn = raw(&place.path);
        assert_eq!(version(&conn), SCHEMA_VERSION as i64);
        assert!(has_column(&conn, "phone", "model") && has_column(&conn, "phone", "app_version"));
    }

    // It goes on: the phone says what it is, kept across a restart; both
    // devices exchange; a code is made.
    let mut hub = Hub::open(&place.path).unwrap();
    assert!(hub.keep_phone_about(&phone.id, "samsung SM-S911B", Some(34), "0.4.0").unwrap());
    hub.close().unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    let said = hub.phone(&phone.id).unwrap();
    assert_eq!((said.android_version, said.model.as_deref(), said.app_version.as_deref()), (Some(34), Some("samsung SM-S911B"), Some("0.4.0")));
    assert_eq!(said.notes, shown.notes, "its notes are as they were");
    use_app(&mut hub, &mut phone, "com.example.game", NOW, NOW + 300);
    exchange(&mut hub, &mut pc, NOW + 310, false);
    hub.make_code(&pc.id, 15, NOW + 320).unwrap();
    assert_eq!(hub.codes_left(&pc.id, NOW + 320).unwrap(), limo_core::CODES_PER_DAY - 3);
    hub.tidy(NOW + 400).unwrap();
}
