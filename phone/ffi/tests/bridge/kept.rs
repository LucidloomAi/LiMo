//! The kept state through this layer: one that cannot be loaded locks the
//! phone and is never written over, and a pairing from bytes that are not a
//! welcome is refused.

use limo_phone_ffi::{CoverKind, EngineError, LoadProblemKind, LockWhy, PhoneEngine};

use crate::common::{Phone, app, at, GAME};

#[test]
fn a_state_that_cannot_be_loaded_locks_the_phone_says_why_and_is_left_as_it_is() {
    let phone = Phone::new(at(12, 0));
    let mut facts = phone.facts();
    facts.front = app(GAME, "Game");

    let newer = String::from_utf8(phone.kept.clone()).unwrap().replacen("format 1", "format 9", 1).into_bytes();
    let mut damaged = phone.kept.clone();
    let last = damaged.len() - 2;
    damaged[last] ^= 0x01;
    let cases = [
        (Vec::new(), LoadProblemKind::Empty, None),
        (b"hello".to_vec(), LoadProblemKind::NotLimo, None),
        (newer, LoadProblemKind::Newer, Some(9)),
        (damaged, LoadProblemKind::Damaged, None),
    ];
    for (bytes, kind, format) in cases {
        let engine = PhoneEngine::load(bytes);
        let problem = engine.load_problem().unwrap().expect("a problem");
        assert_eq!(problem.kind, kind);
        assert_eq!(problem.format, format);
        assert!(problem.message.starts_with("The phone's kept state"), "{}", problem.message);

        let orders = engine.tick(facts.clone()).unwrap();
        assert_eq!(orders.cover.kind, CoverKind::Lock);
        let lock = orders.cover.lock.unwrap();
        assert_eq!(lock.why, LockWhy::CannotCheck);
        assert!(lock.ways.is_empty());
        let nothing = lock.nothing_to_do.expect("words for no way out");
        assert_eq!(nothing.bold, "Ask a guardian.");
        assert!(!orders.save_state && !orders.send_report);
        assert!(!engine.should_save().unwrap());
        assert!(engine.save().unwrap().is_empty());
        assert_eq!(engine.outgoing_report(facts.wall_secs).unwrap(), None);
        assert_eq!(engine.view().unwrap(), None);
        let refused = engine.ask_for_time(10, String::new(), facts.wall_secs).unwrap_err();
        assert!(matches!(refused, EngineError::CannotCheck { .. }));
        assert_eq!(refused.to_string(), "LiMo cannot check the time left just now. Ask a guardian.");
        assert!(matches!(engine.try_code(b"1234 5678".to_vec(), facts.wall_secs), Err(EngineError::CannotCheck { .. })));
    }
}

#[test]
fn a_pairing_from_bytes_that_are_not_a_welcome_or_a_secret_of_the_wrong_length_is_refused() {
    let phone = Phone::new(at(8, 0));
    let welcome = phone.family.welcome(phone.child.clone(), "Second phone".into(), phone.wall).unwrap();
    assert!(matches!(
        PhoneEngine::paired(b"{}".to_vec(), welcome.code_secret.clone(), phone.wall),
        Err(EngineError::NotAWelcome)
    ));
    let short = PhoneEngine::paired(welcome.welcome.clone(), vec![7; 31], phone.wall).unwrap_err();
    assert!(matches!(short, EngineError::SecretWrongLength { .. }), "{short:?}");
    assert!(PhoneEngine::paired(welcome.welcome, welcome.code_secret, phone.wall).is_ok());
}

#[test]
fn a_crash_after_any_call_loses_nothing_kept() {
    let mut phone = Phone::new(at(14, 0));
    for _ in 0..20 {
        phone.run(7);
        phone.crash();
    }
    // With no crash it would be 140 seconds. A crash loses at most the
    // seconds counted since the state was last kept, never more than five,
    // and never counts a second twice.
    let counted = phone.counted();
    assert!((20 * 7 - 20 * 5..=20 * 7).contains(&counted), "{counted}");
}
