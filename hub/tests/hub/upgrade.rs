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
