//! What the agent on the master PC needs: each child's device on the PC,
//! paired once and kept with its state; the exchange that writes the book
//! and the device's state together; the book's revision; the PC's settings;
//! and the events only the agent sees.

use limo_core::{DeviceKind, DeviceState, GrantOutcome, TryOutcome};
use limo_hub::{
    AccountLink, EventKind, Hub, HubError, MasterPcRecord, PcSettings, UnlinkedAccounts, DEFAULT_IDLE_MINUTES,
};

use crate::common::*;

const M: u64 = 60;

fn pc_of(hub: &mut Hub, child_id: &str, now: i64) -> MasterPcRecord {
    hub.pair_master_pc(child_id, "Family PC", now).unwrap()
}

#[test]
fn the_master_pc_is_paired_once_and_its_state_is_kept() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    assert_eq!(hub.master_pc(&asha).unwrap(), None);

    let first = pc_of(&mut hub, &asha, at(7, 30));
    assert_eq!(first.state.device_id(), first.device_id);
    assert_eq!(first.state.child_id(), asha);
    assert!(first.state.has_code_secret() && first.notes.is_empty());
    let devices = hub.devices(&asha).unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!((devices[0].kind, devices[0].platform.as_str()), (DeviceKind::OnMaster, "windows"));

    // Called again, now or after a restart, it is the same device and
    // nothing changes.
    let events = hub.events(100, None).unwrap();
    assert_eq!(pc_of(&mut hub, &asha, at(7, 31)), first);
    assert_eq!(hub.events(100, None).unwrap(), events);
    hub.close().unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.master_pc(&asha).unwrap(), Some(first.clone()));
    assert_eq!(pc_of(&mut hub, &asha, at(7, 32)), first);
    assert_eq!(hub.devices(&asha).unwrap().len(), 1);
    let err = hub.pair_device(&asha, "Another", DeviceKind::OnMaster, "windows", at(7, 33)).unwrap_err();
    assert_eq!(err, HubError::MasterPcAlreadyPaired);

    // The state kept holds the code secret the book has: a code made at the
    // master works on it.
    let mut state = first.state;
    let code = hub.make_code(&first.device_id, 30, at(8, 0)).unwrap();
    assert!(matches!(state.try_code(&code.shown, at(8, 0)), TryOutcome::Added { .. }));
}

#[test]
fn an_exchange_writes_the_book_and_the_device_state_together() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let pc = pc_of(&mut hub, &asha, at(7, 30));
    let mut state = pc.state.clone();

    let done = hub.exchange_on_master_pc(&mut state, true, "first notes", at(8, 0)).unwrap();
    assert_eq!(done.grant.lease_total_secs, 10 * M);
    assert!(done.book_revision > pc.book_revision);
    assert_eq!(state.holding_at(at(8, 0)).lease_total_secs, 10 * M);
    let kept = hub.master_pc(&asha).unwrap().unwrap();
    assert_eq!((kept.state.clone(), kept.notes.as_str(), kept.book_revision), (state.clone(), "first notes", done.book_revision));

    state.record_use("minecraft.exe", at(8, 0), at(8, 5));
    hub.exchange_on_master_pc(&mut state, false, "second", at(8, 5)).unwrap();
    assert_eq!(hub.child_today(&asha, at(8, 5)).unwrap().used_secs, 5 * M);
    assert!(state.unconfirmed_slices().is_empty());
    let today = day_of(at(8, 5));
    assert_eq!(hub.usage(&asha, today, today).unwrap().apps[0].app_id, "minecraft.exe");
    let pc_today = &hub.child_today(&asha, at(8, 5)).unwrap().devices[0];
    assert!(pc_today.in_use);
    hub.close().unwrap();

    // An exchange that fails part way leaves the book, the state kept and
    // the state in hand as they were.
    raw(&place.path)
        .execute_batch("CREATE TRIGGER fail BEFORE INSERT ON slice BEGIN SELECT RAISE(ABORT, 'test failure'); END;")
        .unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    state.record_use("minecraft.exe", at(8, 5), at(8, 7));
    let before = state.clone();
    let today_before = hub.child_today(&asha, at(8, 7)).unwrap();
    let err = hub.exchange_on_master_pc(&mut state, true, "third", at(8, 7)).unwrap_err();
    assert!(matches!(err, HubError::Storage { .. }), "{err:?}");
    assert_eq!(state, before);
    assert_eq!(hub.child_today(&asha, at(8, 7)).unwrap(), today_before);
    assert_eq!(hub.master_pc(&asha).unwrap().unwrap().notes, "second");
    hub.close().unwrap();

    raw(&place.path).execute_batch("DROP TRIGGER fail;").unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    hub.exchange_on_master_pc(&mut state, true, "third", at(8, 8)).unwrap();
    hub.exchange_on_master_pc(&mut state, true, "fourth", at(8, 9)).unwrap();
    assert_eq!(hub.child_today(&asha, at(8, 9)).unwrap().used_secs, 7 * M);
    assert_eq!(hub.usage(&asha, today, today).unwrap().secs, 7 * M);
}

#[test]
fn the_book_revision_grows_with_every_change_to_the_book() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let pc = pc_of(&mut hub, &asha, at(7, 30));
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 31));
    let mut revision = hub.book_revision(&asha).unwrap();
    assert!(revision > pc.book_revision);
    let mut grew = |hub: &Hub, what: &str| {
        let now = hub.book_revision(&asha).unwrap();
        assert!(now > revision, "{what}");
        revision = now;
    };

    hub.give_extra(&asha, 10, "tap-1", at(8, 0)).unwrap();
    grew(&hub, "extra time");
    exchange(&mut hub, &mut phone, at(8, 1), false);
    grew(&hub, "a phone's report");
    hub.lock_now(&asha, None, at(8, 2)).unwrap();
    grew(&hub, "a lock");
    hub.lift(&asha, at(8, 3)).unwrap();
    grew(&hub, "a lift");
    let rules = hub.rules(&asha).unwrap();
    hub.replace_rules(&asha, rules, at(8, 4)).unwrap();
    grew(&hub, "new rules");
    hub.set_pin("482915", at(8, 5)).unwrap();
    grew(&hub, "the PIN");
    // Keeping the PC's state does not touch the book.
    hub.save_master_pc(&pc.state, "notes", at(8, 6)).unwrap();
    assert_eq!(hub.book_revision(&asha).unwrap(), revision);
    assert_eq!(hub.book_revision("child-0000000000000000").unwrap_err(), HubError::UnknownChild {
        child_id: "child-0000000000000000".into()
    });
}

#[test]
fn a_pc_device_paired_before_states_were_kept_is_replaced_once() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut old = pair(&mut hub, &asha, "Old PC", DeviceKind::OnMaster, at(7, 0));
    exchange(&mut hub, &mut old, at(8, 0), true);
    use_app(&mut hub, &mut old, "game", at(8, 0), at(8, 6));
    assert_eq!(hub.master_pc(&asha).unwrap(), None);

    let new = pc_of(&mut hub, &asha, at(9, 0));
    assert_ne!(new.device_id, old.id);
    let devices = hub.devices(&asha).unwrap();
    assert_eq!(devices.iter().map(|d| d.device_id.as_str()).collect::<Vec<_>>(), vec![new.device_id.as_str()]);
    // The old device's use stays counted; its reports are refused.
    assert_eq!(hub.child_today(&asha, at(9, 0)).unwrap().used_secs, 6 * M);
    let err = hub.handle_report(&old.state.report(at(9, 1), false), at(9, 1)).unwrap_err();
    assert_eq!(err, HubError::DeviceRemoved { device_id: old.id.clone() });
    assert!(hub.events(10, None).unwrap().iter().any(|e| e.what == EventKind::DeviceRemoved {
        device_id: old.id.clone(),
        name: "Old PC".into()
    }));
    // And it happens once.
    assert_eq!(pc_of(&mut hub, &asha, at(9, 2)), new);
}

#[test]
fn only_a_kept_pc_device_can_be_saved_and_a_removed_one_takes_its_state_with_it() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let ravi = child(&mut hub, "Ravi").child_id;
    let pc = pc_of(&mut hub, &asha, at(7, 30));
    let mut phone = pair(&mut hub, &asha, "Phone", DeviceKind::Portable, at(7, 30));

    let err = hub.save_master_pc(&phone.state, "", at(8, 0)).unwrap_err();
    assert_eq!(err, HubError::NotMasterPc { device_id: phone.id.clone() });
    plain(&err.to_string());
    let err = hub.exchange_on_master_pc(&mut phone.state, true, "", at(8, 0)).unwrap_err();
    assert_eq!(err, HubError::NotMasterPc { device_id: phone.id.clone() });
    // A PC device paired the old way has no kept state, so it is not one.
    let old = pair(&mut hub, &ravi, "Old PC", DeviceKind::OnMaster, at(7, 30));
    assert_eq!(hub.save_master_pc(&old.state, "", at(8, 0)).unwrap_err(), HubError::NotMasterPc {
        device_id: old.id.clone()
    });

    hub.remove_device(&pc.device_id, at(8, 1)).unwrap();
    hub.close().unwrap();
    assert_eq!(count(&raw(&place.path), "master_pc", "1 = 1"), 0);
    let mut hub = Hub::open(&place.path).unwrap();
    let mut state = pc.state.clone();
    let err = hub.save_master_pc(&state, "", at(8, 2)).unwrap_err();
    assert_eq!(err, HubError::DeviceRemoved { device_id: pc.device_id.clone() });
    let err = hub.exchange_on_master_pc(&mut state, true, "", at(8, 2)).unwrap_err();
    assert_eq!(err, HubError::DeviceRemoved { device_id: pc.device_id.clone() });
    assert_eq!(state, pc.state);
    // The agent then pairs a new one, once.
    let again = pc_of(&mut hub, &asha, at(8, 3));
    assert_ne!(again.device_id, pc.device_id);

    // Removing the child takes the kept state with it.
    hub.remove_child(&asha).unwrap();
    hub.close().unwrap();
    assert_eq!(count(&raw(&place.path), "master_pc", "1 = 1"), 0);
}

#[test]
fn a_grant_taken_on_the_pc_is_the_one_the_book_made() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let mut state = pc_of(&mut hub, &asha, at(7, 30)).state;
    let done = hub.exchange_on_master_pc(&mut state, true, "", at(8, 0)).unwrap();
    // Taking the same grant again is stale: the device took it already.
    let mut copy = state.clone();
    assert_eq!(copy.take_grant(&done.grant, at(8, 0)), GrantOutcome::Stale);
    // What the book holds back for the PC is what the PC holds.
    let today = hub.child_today(&asha, at(8, 0)).unwrap();
    assert_eq!(today.devices[0].holds_secs, Some(10 * M));
    assert_eq!(state.holding_at(at(8, 0)).in_hand_secs(), 10 * M);
}

#[test]
fn the_pc_settings_are_kept_checked_and_reset_with_the_family() {
    let place = place();
    let mut hub = Hub::open(&place.path).unwrap();
    let defaults = PcSettings { idle_minutes: DEFAULT_IDLE_MINUTES, unlinked_accounts: UnlinkedAccounts::Free };
    assert_eq!(hub.pc_settings().unwrap(), defaults);
    // No family yet: an account that is not linked cannot be locked, so a PC
    // being set up is never locked.
    assert_eq!(hub.set_unlinked_accounts(UnlinkedAccounts::Locked).unwrap_err(), HubError::NoFamily);
    hub.create_family(PASSWORD, at(7, 0)).unwrap();
    hub.set_unlinked_accounts(UnlinkedAccounts::Locked).unwrap();
    for bad in [0, 61, u32::MAX] {
        let err = hub.set_idle_minutes(bad).unwrap_err();
        assert_eq!(err, HubError::SettingOutOfRange { min: 1, max: 60 });
        plain(&err.to_string());
    }
    hub.set_idle_minutes(10).unwrap();
    hub.close().unwrap();
    let mut hub = Hub::open(&place.path).unwrap();
    assert_eq!(hub.pc_settings().unwrap(), PcSettings { idle_minutes: 10, unlinked_accounts: UnlinkedAccounts::Locked });
    hub.delete_family().unwrap();
    assert_eq!(hub.pc_settings().unwrap(), defaults);
}

#[test]
fn notes_for_the_guardian_are_kept_once_where_once_is_asked() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    let pc = pc_of(&mut hub, &asha, at(7, 30));
    let today = day_of(at(8, 0));
    hub.link_account("S-1-5-21-1001", "asha", AccountLink::Child { child_id: asha.clone() }).unwrap();

    assert!(hub.note_unlinked_account_used("S-1-5-21-1009", "Visitor", today, at(8, 0)).unwrap());
    assert!(!hub.note_unlinked_account_used("S-1-5-21-1009", "Visitor", today, at(9, 0)).unwrap());
    assert!(hub.note_unlinked_account_used("S-1-5-21-1009", "Visitor", today + 1, at(9, 0) + 86_400).unwrap());
    assert!(hub.note_blocked_program_closed(&pc.device_id, "robloxplayerbeta.exe", today, at(8, 1)).unwrap());
    assert!(!hub.note_blocked_program_closed(&pc.device_id, "robloxplayerbeta.exe", today, at(8, 2)).unwrap());
    assert!(hub.note_blocked_program_closed(&pc.device_id, "fortnite.exe", today, at(8, 3)).unwrap());
    hub.note_clock_jump(-3_600, at(8, 4)).unwrap();
    hub.note_time_zone_change(19_800, 0, at(8, 5)).unwrap();
    assert_eq!(
        hub.note_blocked_program_closed(&pc.device_id, "", today, at(8, 6)).unwrap_err(),
        HubError::TextEmpty { field: limo_hub::TextField::ProgramName }
    );
    assert!(matches!(
        hub.note_blocked_program_closed("device-0000000000000000", "x.exe", today, at(8, 6)),
        Err(HubError::UnknownDevice { .. })
    ));

    let events = hub.events(6, None).unwrap();
    let kinds: Vec<(Option<String>, EventKind)> = events.into_iter().map(|e| (e.child_id, e.what)).collect();
    assert_eq!(kinds, vec![
        (None, EventKind::TimeZoneChanged { from_offset_secs: 19_800, to_offset_secs: 0 }),
        (None, EventKind::ClockJumped { by_secs: -3_600 }),
        (Some(asha.clone()), EventKind::BlockedProgramClosed {
            device_id: pc.device_id.clone(),
            program: "fortnite.exe".into(),
            day: today
        }),
        (Some(asha.clone()), EventKind::BlockedProgramClosed {
            device_id: pc.device_id.clone(),
            program: "robloxplayerbeta.exe".into(),
            day: today
        }),
        (None, EventKind::UnlinkedAccountUsed {
            account_id: "S-1-5-21-1009".into(),
            account_name: "Visitor".into(),
            day: today + 1
        }),
        (None, EventKind::UnlinkedAccountUsed {
            account_id: "S-1-5-21-1009".into(),
            account_name: "Visitor".into(),
            day: today
        }),
    ]);
}

#[test]
fn the_kept_state_hides_its_secrets_in_debug() {
    let place = place();
    let mut hub = new_family(&place.path);
    let asha = child(&mut hub, "Asha").child_id;
    hub.set_pin("482915", at(7, 1)).unwrap();
    let mut pc = pc_of(&mut hub, &asha, at(7, 30));
    hub.exchange_on_master_pc(&mut pc.state, false, "", at(7, 31)).unwrap();
    let pc = hub.master_pc(&asha).unwrap().unwrap();
    assert!(pc.state.has_pin() && pc.state.has_code_secret());
    let shown = format!("{pc:?}");
    assert!(shown.contains("hidden"), "{shown}");
    // The secret is in the saved form, and not in what Debug shows.
    let saved: serde_json::Value = serde_json::to_value(&pc.state).unwrap();
    let secret = saved["code_secret"].to_string();
    assert!(secret.len() > 40, "{secret}");
    let bytes: Vec<u8> = serde_json::from_value(saved["code_secret"].clone()).unwrap();
    let as_debug = format!("{bytes:?}");
    assert!(!shown.contains(&as_debug) && !shown.contains(&secret), "{shown}");
    let restored: DeviceState = serde_json::from_value(saved).unwrap();
    assert_eq!(restored, pc.state);
}
