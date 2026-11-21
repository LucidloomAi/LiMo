//! Notes and the other devices' use, through this layer only: the notes
//! cross in the report's bytes with their ids, a grant's bytes confirm them
//! and the phone drops them, and the view's JSON says how much of the day
//! went on the PC once the master says it.

use limo_phone_ffi::GrantOutcome;

use crate::common::{CASINO, GAME, Phone, all_permissions, app, at};

#[test]
fn notes_cross_in_the_reports_bytes_and_a_grant_confirms_them() {
    let mut phone = Phone::new(at(16, 0));
    // Away: accessibility switched off a moment, and the blocked casino.
    phone.run(5);
    phone.permissions.accessibility = false;
    phone.run(1);
    phone.permissions = all_permissions();
    phone.front = app(CASINO, "Casino");
    phone.run(1);
    phone.front = app(GAME, "Game");
    phone.run(1);
    let view = phone.engine.view().unwrap().unwrap();
    // Setup finished; how the phone stood at the first call (six steps on,
    // base protection); accessibility off and the protection with it, then
    // on again (each taking the place of the older note that said the
    // same); the casino.
    assert_eq!(view.notes_kept, 11);

    // At home: the report carries the ten the master takes, as the core's
    // JSON, each with its id.
    phone.in_reach = true;
    phone.wall += 1;
    phone.elapsed_ms += 1_000;
    assert!(phone.engine.tick(phone.facts()).unwrap().send_report);
    let report = phone.engine.outgoing_report(phone.wall).unwrap().unwrap();
    phone.kept = phone.engine.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&report).unwrap();
    let ids: Vec<&str> = json["notes"].as_array().unwrap().iter().map(|n| n["id"].as_str().unwrap()).collect();
    let device = phone.device_id.clone();
    assert_eq!(ids, (1..=10).map(|n| format!("{device}/note/{n}")).collect::<Vec<_>>());
    assert_eq!(json["notes"][0]["note"]["note"], "setup_finished");
    assert_eq!(json["notes"][6]["note"], serde_json::json!({ "note": "permission_off", "permission": "accessibility", "at": json["notes"][6]["note"]["at"] }));
    assert_eq!(json["notes"][9]["note"], serde_json::json!({ "note": "protection", "level": "base", "at": json["notes"][9]["note"]["at"] }));
    assert!(!String::from_utf8_lossy(&report).contains("blocked_app_opened"), "it stays on the phone");

    // The grant's bytes confirm them: dropped, the casino's note kept.
    let grant = phone.family.answer(report, phone.wall).unwrap();
    let confirmed: serde_json::Value = serde_json::from_slice(&grant).unwrap();
    assert_eq!(confirmed["confirmed_notes"], serde_json::json!(ids));
    assert_eq!(phone.engine.take_grant(grant, phone.wall).unwrap().outcome, GrantOutcome::Taken);
    assert!(phone.engine.should_save().unwrap());
    assert_eq!(phone.engine.view().unwrap().unwrap().notes_kept, 1);
}

#[test]
fn the_views_json_says_how_much_went_on_the_pc() {
    let mut phone = Phone::new(at(16, 0));
    // 12 minutes on the family PC, then the phone at home for two.
    phone.family.use_pc(phone.child.clone(), 12, at(16, 0)).unwrap();
    phone.in_reach = true;
    phone.run(2 * 60);
    phone.screen_on = false;
    phone.run(65);
    let mine = phone.counted() / 60;
    let json = phone.engine.view().unwrap().unwrap().json;
    let where_used = format!(r#""where_used":"On this phone {mine} min · on the PC 12 min""#);
    assert!(json.contains(&where_used), "{where_used} is not in {json}");
    let used = format!(r#""used":"{} min used of 30 min""#, mine + 12);
    assert!(json.contains(&used), "{used} is not in {json}");
}
