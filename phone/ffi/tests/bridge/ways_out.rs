//! The three ways out, through this layer: their answers as records, and
//! the core's refusals as their own errors.

use limo_phone_ffi::{EngineError, PinAction, PinActionKind, RequestAnswer, TryKind};

use crate::common::{PIN, Phone, at};

fn pin(kind: PinActionKind, minutes: Option<i32>, until: Option<i64>) -> PinAction {
    PinAction { kind, minutes, until }
}

#[test]
fn asking_for_time_gives_the_request_and_the_core_refuses_a_second() {
    let mut phone = Phone::new(at(17, 0));
    phone.run(2);
    let request = phone.engine.ask_for_time(20, "For the school project".into(), phone.wall).unwrap();
    assert_eq!(request.minutes, 20);
    assert_eq!(request.reason, "For the school project");
    assert_eq!(request.device_id, phone.device_id);
    assert_eq!(request.child_id, phone.child);
    assert!(phone.engine.should_save().unwrap());
    phone.kept = phone.engine.save().unwrap();

    let again = phone.engine.ask_for_time(10, String::new(), phone.wall).unwrap_err();
    assert!(matches!(again, EngineError::RequestWaiting { .. }), "{again:?}");
    assert_eq!(again.to_string(), "A request from this device is still waiting for an answer.");
    assert!(matches!(phone.engine.ask_for_time(-1, String::new(), phone.wall), Err(EngineError::OutOfRange { .. })));

    // The lock shows it, with its answer so far.
    phone.run(30 * 60);
    let lock = phone.orders.clone().unwrap().cover.lock.expect("the lock");
    let latest = lock.latest_request.expect("the request");
    assert_eq!(latest.request.minutes, 20);
    assert_eq!(latest.answer, RequestAnswer::Waiting);
    assert_eq!(latest.granted_minutes, None);
    assert!(!latest.reached_master);
    assert_eq!(lock.request_line.expect("its line").stamp, "Waiting");
}

#[test]
fn codes_wrong_and_malformed_and_the_wait_after_five() {
    let mut phone = Phone::new(at(17, 0));
    phone.run(1);
    let wrong = phone.engine.try_code(b"0000 0000".to_vec(), phone.wall).unwrap();
    assert_eq!(wrong.kind, TryKind::Wrong);
    assert_eq!(wrong.wrong_guesses, Some(1));
    assert_eq!(wrong.wait_until, None);
    assert_eq!(wrong.message, "That code is not right. 4 more tries before a wait.");
    assert_eq!(wrong.opens_now, None);

    let short = phone.engine.try_code(b"12".to_vec(), phone.wall).unwrap();
    assert_eq!(short.kind, TryKind::Malformed);
    assert_eq!(short.message, "A code has 8 digits.");

    // Bytes that are not text cannot be a code.
    assert!(matches!(phone.engine.try_code(vec![0xff, 0x00], phone.wall), Err(EngineError::OutOfRange { .. })));

    let mut last = wrong;
    for _ in 0..4 {
        last = phone.engine.try_code(b"0000 0000".to_vec(), phone.wall).unwrap();
    }
    assert_eq!(last.kind, TryKind::Wrong);
    assert_eq!(last.wrong_guesses, Some(5));
    let wait = last.wait_until.expect("a wait after five");
    assert!(wait > phone.wall);
    let waiting = phone.engine.try_code(b"0000 0000".to_vec(), phone.wall).unwrap();
    assert_eq!(waiting.kind, TryKind::MustWait);
    assert_eq!(waiting.wait_until, Some(wait));

    // The wait survives a crash, from the kept state.
    phone.kept = phone.engine.save().unwrap();
    phone.crash();
    let still = phone.engine.try_code(b"0000 0000".to_vec(), phone.wall).unwrap();
    assert_eq!(still.kind, TryKind::MustWait);
}

#[test]
fn the_pin_adds_unlocks_and_confirms_a_release() {
    let mut phone = Phone::new(at(19, 0));
    phone.run(1);
    let added = phone
        .engine
        .try_pin(PIN.as_bytes().to_vec(), pin(PinActionKind::AddMinutes, Some(20), None), phone.wall)
        .unwrap();
    assert_eq!(added.kind, TryKind::Added, "{added:?}");
    assert_eq!(added.added_secs, Some(20 * 60));

    let until = phone.wall + 3_600;
    let unlocked =
        phone.engine.try_pin(PIN.as_bytes().to_vec(), pin(PinActionKind::UnlockUntil, None, Some(until)), phone.wall).unwrap();
    assert_eq!(unlocked.kind, TryKind::Unlocked);
    assert_eq!(unlocked.unlocked_until, Some(until));
    assert_eq!(phone.run(1).time_left.unlocked_until, Some(until));

    let release =
        phone.engine.try_pin(PIN.as_bytes().to_vec(), pin(PinActionKind::ConfirmRelease, None, None), phone.wall).unwrap();
    assert_eq!(release.kind, TryKind::ReleaseConfirmed);

    let wrong =
        phone.engine.try_pin(b"111111".to_vec(), pin(PinActionKind::ConfirmRelease, None, None), phone.wall).unwrap();
    assert_eq!(wrong.kind, TryKind::Wrong);
    assert_eq!(wrong.wrong_guesses, Some(1));

    // An action that cannot be done is refused before the PIN is looked
    // at, in the core's words; one missing what its kind needs, by this
    // layer.
    let none = phone.engine.try_pin(PIN.as_bytes().to_vec(), pin(PinActionKind::AddMinutes, Some(0), None), phone.wall);
    assert!(matches!(none, Err(EngineError::MinutesOutOfRange { .. })), "{none:?}");
    let missing = phone.engine.try_pin(PIN.as_bytes().to_vec(), pin(PinActionKind::AddMinutes, None, None), phone.wall);
    assert!(matches!(missing, Err(EngineError::OutOfRange { .. })));
    let past = phone.engine.try_pin(
        PIN.as_bytes().to_vec(),
        pin(PinActionKind::UnlockUntil, None, Some(phone.wall - 1)),
        phone.wall,
    );
    assert!(matches!(past, Err(EngineError::UnlockOutOfRange { .. })), "{past:?}");
}
