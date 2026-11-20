//! A short day, through this layer only: the same story the JVM tests in
//! the app's child module play from Kotlin.

use limo_phone_ffi::{
    CoverKind, FrontKind, GrantOutcome, LockWhy, PhoneEngine, PhoneOrders, TryKind, Way,
};

use crate::common::{CASINO, CLOCK, GAME, Phone, app, at, screen};

/// The orders with what only the engine that kept the state can know
/// (whether to keep it again now) left out.
fn comparable(orders: &PhoneOrders) -> PhoneOrders {
    PhoneOrders { save_state: false, ..orders.clone() }
}

#[test]
fn a_short_day_from_a_made_up_welcome() {
    let mut phone = Phone::new(at(16, 0));

    // Counting in a game, out of reach of the master, on the away time.
    let orders = phone.run(60);
    assert_eq!(orders.cover.kind, CoverKind::Nothing);
    assert!(orders.time_left.counting);
    assert!(orders.time_left.away);
    assert_eq!(orders.time_left.seconds_left, Some(30 * 60 - 60));
    assert_eq!(orders.time_left.words, "29 min left");
    assert_eq!(phone.counted(), 60);
    assert!(!orders.send_report, "a report goes only in reach");

    // A blocked app is covered, named by its label, and does not count.
    phone.front = app(CASINO, "Casino");
    let orders = phone.run(10);
    assert_eq!(orders.cover.kind, CoverKind::Block);
    let block = orders.cover.block.clone().unwrap();
    assert_eq!(block.package_name, CASINO);
    assert_eq!(block.title, "Casino is not allowed.");
    assert_eq!(block.message, "It is blocked by your family's rules.");
    assert!(block.go_home);
    assert!(!block.call_on);
    assert!(!orders.time_left.counting);
    assert_eq!(phone.counted(), 60);
    let noted: Vec<_> = phone.notes.iter().filter(|n| n.contains("blocked_app_opened")).collect();
    assert_eq!(noted.len(), 1, "{:?}", phone.notes);
    assert!(noted[0].contains(CASINO));
    assert!(!noted[0].contains("Casino\""), "the label is never kept or noted");

    // Back to the game until the time is used up: warnings on the way, then
    // the lock, which names the clock as still open and offers a code. (The
    // first second after the casino counts from the call that brought the
    // game back.)
    phone.front = app(GAME, "Game");
    let mut warnings = Vec::new();
    let mut orders = phone.tick();
    while orders.cover.kind == CoverKind::Nothing {
        phone.wall += 1;
        phone.elapsed_ms += 1_000;
        orders = phone.tick();
        if let Some(w) = &orders.warning {
            warnings.push(w.minutes);
        }
    }
    assert_eq!(warnings, vec![15, 5, 1]);
    assert_eq!(phone.counted(), 30 * 60);
    assert_eq!(orders.cover.kind, CoverKind::Lock);
    let lock = orders.cover.lock.clone().unwrap();
    // The whole day was the away allowance, so the day is what ran out.
    assert_eq!(lock.why, LockWhy::TimeUsedUp);
    assert_eq!(lock.title, "Today's time is used up.");
    assert_eq!(lock.allowed_packages, vec![CLOCK.to_string()]);
    assert!(lock.can_use_code);
    assert!(lock.can_use_pin);
    let ways: Vec<Way> = lock.ways.iter().map(|w| w.way).collect();
    assert!(ways.contains(&Way::Code) && ways.contains(&Way::Pin), "{ways:?}");
    assert!(orders.time_left.locked);
    assert!(!orders.time_left.counting);
    assert_eq!(phone.counted(), 30 * 60);

    // The clock stays usable under the lock, and does not count.
    phone.front = app(CLOCK, "Clock");
    let orders = phone.run(5);
    assert_eq!(orders.cover.kind, CoverKind::Nothing);
    assert_eq!(phone.counted(), 30 * 60);
    // The dialler and LiMo itself are never covered.
    for kind in [FrontKind::Dialler, FrontKind::EmergencyDialler, FrontKind::Limo] {
        phone.front = screen(kind);
        assert_eq!(phone.tick().cover.kind, CoverKind::Nothing, "{kind:?}");
    }
    // The home screen is.
    phone.front = screen(FrontKind::Home);
    assert_eq!(phone.tick().cover.kind, CoverKind::Lock);

    // A right code, read out by a guardian who is away, opens it.
    phone.front = screen(FrontKind::Limo);
    let code = phone.family.make_code(phone.device_id.clone(), 15, phone.wall).unwrap();
    let answer = phone.engine.try_code(code.clone().into_bytes(), phone.wall).unwrap();
    assert_eq!(answer.kind, TryKind::Added);
    assert_eq!(answer.added_secs, Some(15 * 60));
    assert_eq!(answer.message, "15 minutes added.");
    assert_eq!(answer.opens_now.as_deref(), Some("The phone opens now."));
    assert!(phone.engine.should_save().unwrap());
    phone.kept = phone.engine.save().unwrap();
    // The same code again is not counted as a wrong guess.
    let again = phone.engine.try_code(code.into_bytes(), phone.wall).unwrap();
    assert_eq!(again.kind, TryKind::AlreadyUsed);
    phone.kept = phone.engine.save().unwrap();

    phone.front = app(GAME, "Game");
    let orders = phone.run(30);
    assert_eq!(orders.cover.kind, CoverKind::Nothing);
    assert!(orders.time_left.counting);
    assert_eq!(phone.counted(), 30 * 60 + 30);

    // The state kept, and a new engine loaded from it, gives the same
    // orders as the one that kept it.
    phone.kept = phone.engine.save().unwrap();
    let reloaded = PhoneEngine::load(phone.kept.clone());
    assert_eq!(reloaded.load_problem().unwrap(), None);
    phone.wall += 1;
    phone.elapsed_ms += 1_000;
    let before = phone.engine.tick(phone.facts()).unwrap();
    let after = reloaded.tick(phone.facts()).unwrap();
    assert_eq!(comparable(&after), comparable(&before));
    // A reloaded engine asks to be kept at once: it cannot know when its
    // bytes were last written.
    assert!(after.save_state);
    assert_eq!(reloaded.view().unwrap().unwrap().counted_secs, phone.counted());

    // The day goes on from the reloaded engine.
    phone.engine = reloaded;
    let orders = phone.run(60);
    assert!(orders.time_left.counting);
    assert_eq!(phone.counted(), 30 * 60 + 31 + 60);
}

#[test]
fn in_reach_the_report_goes_out_and_the_grant_comes_back_as_bytes() {
    let mut phone = Phone::new(at(9, 0));
    phone.in_reach = true;
    let orders = phone.run(3);
    assert!(phone.reports >= 1, "the first call in reach reports");
    assert!(!orders.time_left.away);
    assert!(orders.time_left.counting);

    // The report is the core's own, in JSON.
    let report = phone.engine.outgoing_report(phone.wall).unwrap().unwrap();
    let parsed: limo_core::Report = serde_json::from_slice(&report).unwrap();
    assert_eq!(parsed.device_id, phone.device_id);
    assert_eq!(parsed.child_id, phone.child);
    // The grant that answers it, too.
    let grant = phone.family.answer(report, phone.wall).unwrap();
    let taken = phone.engine.take_grant(grant.clone(), phone.wall).unwrap();
    assert_eq!(taken.outcome, GrantOutcome::Taken);
    assert!(phone.engine.should_save().unwrap());
    // The same grant again is stale.
    assert_eq!(phone.engine.take_grant(grant, phone.wall).unwrap().outcome, GrantOutcome::Stale);

    // Bytes that are not a grant are refused, and change nothing.
    assert_eq!(phone.engine.take_grant(b"not a grant".to_vec(), phone.wall), Err(limo_phone_ffi::EngineError::NotAGrant));
    assert_eq!(phone.engine.take_grant(Vec::new(), phone.wall), Err(limo_phone_ffi::EngineError::NotAGrant));

    // Out of reach after a failed exchange.
    phone.engine.lost_contact().unwrap();
    phone.in_reach = false;
    assert!(phone.tick().time_left.away);
}

#[test]
fn the_notes_cross_as_the_engines_own_json() {
    let mut phone = Phone::new(at(10, 0));
    phone.run(2);
    // Someone sets the clock forward an hour, with automatic time off.
    phone.auto_time = false;
    phone.wall += 3_600;
    phone.run(1);
    let moved: Vec<limo_phone::PhoneNote> = phone
        .notes
        .iter()
        .map(|n| serde_json::from_str(n).unwrap())
        .filter(|n| matches!(n, limo_phone::PhoneNote::ClockMoved { .. }))
        .collect();
    assert_eq!(moved.len(), 1, "{:?}", phone.notes);
    assert!(matches!(moved[0], limo_phone::PhoneNote::ClockMoved { by_secs: 3_600, followed: false, .. }));
    let view = phone.engine.view().unwrap().unwrap();
    assert_eq!(view.clock_correction_secs, 3_600);
    assert!(view.notes_kept >= 1);
    // The whole view, for the child's own screen, is the engine's JSON.
    let whole: limo_phone::PhoneView = serde_json::from_str(&view.json).unwrap();
    assert_eq!(whole.device_id, phone.device_id);
    assert_eq!(whole.counted_secs as i64, view.counted_secs);
}

#[test]
fn the_views_json_carries_today_the_rules_in_words_and_what_is_switched_off() {
    let mut phone = Phone::new(at(16, 0));
    phone.run(5 * 60);
    phone.front = app(CLOCK, "Clock");
    phone.run(2 * 60);
    phone.front = screen(FrontKind::Limo);
    phone.permissions.phone = false;
    phone.run(1);
    let view = phone.engine.view().unwrap().unwrap();
    // The names Kotlin reads, as they are in the JSON.
    let json: serde_json::Value = serde_json::from_str(&view.json).unwrap();
    let today = &json["today"];
    assert_eq!(today["left"], serde_json::json!({ "kind": "left", "words": "25 min" }));
    assert_eq!(today["used"], "5 min used of 30 min");
    assert_eq!(today["used_part"], 0.167);
    assert_eq!(today["where_used"], "On this phone 5 min");
    assert_eq!(today["bedtime"], serde_json::Value::Null);
    assert_eq!(
        today["apps"],
        serde_json::json!([
            { "package": GAME, "secs": 300, "minutes": "5 min" },
            { "package": CLOCK, "secs": 120, "minutes": "2 min" },
        ])
    );
    assert_eq!(
        today["guardian_needed"],
        serde_json::json!({ "off": [
            { "permission": "phone", "words": "Phone access is switched off, so a lock could hide a call." }
        ] })
    );
    assert_eq!(
        json["rules_words"],
        serde_json::json!({
            "time_a_day": "30 min",
            "screens_off": null,
            "always_open": [CLOCK],
            "not_allowed": [CASINO],
        })
    );
    // A label never reaches the view.
    assert!(!view.json.contains("\"Clock\"") && !view.json.contains("\"Game\""));
    // All on again: no guardian needed.
    phone.permissions = crate::common::all_permissions();
    phone.run(1);
    let json: serde_json::Value = serde_json::from_str(&phone.engine.view().unwrap().unwrap().json).unwrap();
    assert_eq!(json["today"]["guardian_needed"], serde_json::Value::Null);
}

#[test]
fn a_ringing_call_is_never_covered_and_a_call_in_progress_offers_the_way_back() {
    let mut phone = Phone::new(at(18, 0));
    phone.front = app(CASINO, "Casino");
    phone.call = limo_phone_ffi::CallState::Ringing;
    assert_eq!(phone.tick().cover.kind, CoverKind::Nothing);
    phone.call = limo_phone_ffi::CallState::Ongoing;
    let orders = phone.tick();
    let block = orders.cover.block.unwrap();
    assert!(block.call_on);
    assert_eq!(block.back_to_call.as_deref(), Some("Back to the call"));
}
