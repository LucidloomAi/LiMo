//! The kept state: written and read back the same; and a state that cannot
//! be loaded (empty, not LiMo's, from a newer LiMo, cut short, changed)
//! failing safe: the phone locked with LiMo cannot check the time left,
//! nothing ever covered over a call or the diallers, no way out, no report,
//! and the bytes never overwritten. A state in the first form with only the
//! fields that form must have loads.

use limo_core::{DailyBudget, PinAction, WeekBudgets};
use limo_phone::{CallState, Cover, Front, LoadProblem, LockWhy, PhoneEngine, PhoneError, PhoneFacts, STATE_FORMAT};

use crate::common::*;

/// A phone with a day's use, a note and a warning behind it, kept.
fn used_phone() -> Family {
    let mut f = Family::new(at(16, 0));
    f.set_rules(|r| r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(30)));
    f.run(12 * MIN);
    f.offset = 0;
    f.run(5);
    f.screen_on = false;
    f.tick();
    f
}

#[test]
fn a_kept_state_reads_back_the_same_and_goes_on_the_same() {
    let mut f = used_phone();
    let bytes = f.kept.clone();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("limo-phone-state"));
    assert_eq!(lines.next(), Some(format!("format {STATE_FORMAT}").as_str()));
    assert!(lines.next().unwrap().starts_with("check "));
    let mut again = PhoneEngine::load(&bytes);
    assert_eq!(again.load_problem(), None);
    assert!(!again.should_save());
    assert_eq!(again.save(), bytes);
    // Two engines from the same bytes give the same orders.
    let facts = PhoneFacts { interactive: true, ..f.facts() };
    let mut other = PhoneEngine::load(&bytes);
    assert_eq!(again.tick(&facts), other.tick(&facts));
    f.crash();
    f.tick();
    assert_eq!(f.counted(), 12 * 60 + 5);
}

#[test]
fn an_apps_label_is_shown_but_never_kept_nor_reported() {
    let mut f = Family::new(at(16, 0));
    f.in_reach = true;
    f.set_rules(|r| r.blocked_apps = vec![CASINO.into()]);
    // A counted app and a blocked one, each with its label, at home: used,
    // blocked, kept and reported.
    f.front = Front::labelled(GAME, "Rocket Garden");
    f.run(2 * MIN);
    f.front = Front::labelled(CASINO, "Lucky Spins");
    f.run(5);
    assert!(matches!(&f.orders.cover, Cover::Block { title, .. } if title == "Lucky Spins is not allowed."));
    f.front = Front::labelled(GAME, "Rocket Garden");
    f.run(2 * MIN);
    assert!(f.reports > 0);
    let wall = f.wall();
    let report = f.engine().outgoing_report(wall).unwrap();
    let report = serde_json::to_string(&report).unwrap();
    let kept = String::from_utf8(f.engine().save()).unwrap();
    let view = serde_json::to_string(&f.view()).unwrap();
    for label in ["Rocket", "Garden", "Lucky", "Spins"] {
        assert!(!report.contains(label), "{label} in a report");
        assert!(!kept.contains(label), "{label} in the kept state");
        assert!(!view.contains(label), "{label} in the view");
    }
    // The master has the packages only.
    let (child, day) = (f.child.clone(), day_of(f.now()));
    let usage = f.hub().usage(&child, day, day).unwrap();
    assert!(usage.apps.iter().any(|a| a.app_id == GAME));
    assert!(usage.apps.iter().all(|a| !a.app_id.contains("Rocket")));
}

#[test]
fn a_save_that_could_not_be_written_is_asked_for_again() {
    let mut f = used_phone();
    f.screen_on = true;
    f.run(3);
    let engine = f.engine();
    engine.save();
    assert!(!engine.should_save());
    engine.save_failed();
    assert!(engine.should_save());
    let facts = f.facts();
    assert!(f.engine().tick(&facts).save_state);
}

#[test]
fn a_state_that_cannot_be_loaded_locks_the_phone_and_is_left_as_it_is() {
    let f = used_phone();
    let good = f.kept.clone();
    let text = String::from_utf8(good.clone()).unwrap();
    let mut changed = good.clone();
    let last = changed.len() - 3;
    changed[last] ^= 0x01;
    let newer = text.replacen("format 1", "format 2", 1).into_bytes();
    let unchecked = text.replacen("check ", "check 0", 1).into_bytes();
    let cases: Vec<(Vec<u8>, LoadProblem)> = vec![
        (Vec::new(), LoadProblem::Empty),
        (b"{\"state\":{}}".to_vec(), LoadProblem::NotLiMo),
        (vec![0xff, 0xfe, 0x00, 0x01], LoadProblem::NotLiMo),
        (good[..good.len() / 2].to_vec(), LoadProblem::Damaged),
        (changed, LoadProblem::Damaged),
        (unchecked, LoadProblem::Damaged),
        (newer, LoadProblem::Newer { format: 2 }),
        (b"limo-phone-state\nformat 0\ncheck 0000000000000000\n{}".to_vec(), LoadProblem::Damaged),
    ];
    for (bytes, problem) in cases {
        let mut engine = PhoneEngine::load(&bytes);
        assert_eq!(engine.load_problem(), Some(problem), "{problem:?}");
        let facts = PhoneFacts { interactive: true, in_reach: true, prodded: true, ..f.facts() };
        let orders = engine.tick(&facts);
        let Cover::Lock(lock) = &orders.cover else { panic!("not locked for {problem:?}") };
        assert_eq!(lock.why, LockWhy::CannotCheck);
        assert_eq!(lock.message, "LiMo cannot check the time left just now. Ask a guardian.");
        assert!(lock.ways.is_empty() && lock.allowed_packages.is_empty());
        assert_eq!(
            lock.nothing_to_do,
            Some(("Ask a guardian.".to_string(), "A guardian can look at LiMo on the family PC to see what is wrong.".to_string()))
        );
        assert!(!orders.send_report && !orders.save_state && orders.time_left.locked);
        // Never over a call, the diallers or LiMo.
        for front in [Front::Dialler, Front::EmergencyDialler, Front::LiMo] {
            let orders = engine.tick(&PhoneFacts { front, ..facts.clone() });
            assert_eq!(orders.cover, Cover::Nothing);
        }
        let orders = engine.tick(&PhoneFacts { call: CallState::Ringing, ..facts.clone() });
        assert_eq!(orders.cover, Cover::Nothing);
        // No way out, no report, nothing to keep.
        assert_eq!(engine.try_code("1234 5678", facts.wall_secs), Err(PhoneError::CannotCheck));
        assert_eq!(
            engine.try_pin(PIN, PinAction::AddMinutes { minutes: 10 }, facts.wall_secs),
            Err(PhoneError::CannotCheck)
        );
        assert_eq!(engine.ask_for_time(10, "", facts.wall_secs), Err(PhoneError::CannotCheck));
        assert_eq!(engine.outgoing_report(facts.wall_secs), None);
        assert!(!engine.should_save());
        assert!(engine.save().is_empty());
        assert!(engine.view().is_none());
    }
    assert_eq!(
        LoadProblem::Newer { format: 2 }.to_string(),
        "The phone's kept state was written by a newer LiMo (form 2); this one reads up to form 1."
    );
}

#[test]
fn a_state_in_the_first_form_with_only_its_needed_fields_loads() {
    let f = used_phone();
    let text = String::from_utf8(f.kept.clone()).unwrap();
    let body = text.splitn(4, '\n').nth(3).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(body).unwrap();
    let object = value.as_object_mut().unwrap();
    // Only the device state is needed; every other field is filled in.
    object.retain(|key, _| key == "state");
    let body = serde_json::to_string(&value).unwrap();
    let blob = format!("limo-phone-state\nformat 1\ncheck {:016x}\n{body}", fnv(body.as_bytes()));
    let mut engine = PhoneEngine::load(blob.as_bytes());
    assert_eq!(engine.load_problem(), None);
    let orders = engine.tick(&PhoneFacts { interactive: true, ..f.facts() });
    assert!(!matches!(orders.cover, Cover::Lock(_)));
    assert_eq!(engine.view().unwrap().counted_secs, f.view().counted_secs);
}

/// FNV-1a, 64 bits, as the kept state's check.
fn fnv(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
