//! The guard through this layer: a guarded page in the facts crosses in, the
//! guard crosses out as its own record, and the PIN on it answers as a
//! record, wrong PINs counted with the lock's.

use limo_phone_ffi::{CoverKind, EngineError, GuardedPage, TryKind, Way};

use crate::common::{PIN, Phone, app, at};

/// Android's Settings, as a sample name.
const SETTINGS: &str = "org.example.settings";

#[test]
fn a_guarded_page_is_covered_by_the_guard_and_the_pin_opens_it() {
    let mut phone = Phone::new(at(17, 0));
    phone.front = app(SETTINGS, "Settings");
    phone.run(1);
    assert_eq!(phone.orders.clone().unwrap().cover.kind, CoverKind::Nothing);

    phone.guarded = Some(GuardedPage::DeviceAdmin);
    let orders = phone.run(1);
    assert_eq!(orders.cover.kind, CoverKind::Guard);
    assert!(orders.cover.block.is_none() && orders.cover.lock.is_none());
    let guard = orders.cover.guard.expect("the guard");
    assert_eq!(guard.page, GuardedPage::DeviceAdmin);
    assert_eq!(guard.stamp, "Protected");
    assert_eq!(guard.title, "A guardian's PIN is needed here.");
    assert_eq!(guard.message, "These settings can switch LiMo off or remove it from this phone.");
    assert!(guard.can_use_pin);
    assert_eq!(guard.ways.len(), 1);
    assert_eq!(guard.ways[0].way, Way::Pin);
    assert_eq!(guard.ways[0].sub, "Opens these settings for 5 minutes");
    assert_eq!(guard.nothing_to_do, None);
    assert!(!guard.call_on && guard.back_to_call.is_none());
    assert!(phone.notes.iter().any(|n| n.contains("\"note\":\"guarded_page_opened\"") && n.contains("\"device_admin\"")));

    // A wrong PIN, then the right one: its kind and words cross as records.
    let wrong = phone.engine.try_guard_pin(b"000000".to_vec(), phone.wall).unwrap();
    assert_eq!(wrong.kind, TryKind::Wrong);
    assert_eq!(wrong.wrong_guesses, Some(1));
    assert_eq!(wrong.message, "That PIN is not right. 4 more tries before a wait.");
    assert!(phone.engine.should_save().unwrap());
    phone.kept = phone.engine.save().unwrap();
    let right = phone.engine.try_guard_pin(PIN.as_bytes().to_vec(), phone.wall).unwrap();
    assert_eq!(right.kind, TryKind::ReleaseConfirmed);
    // Two seconds after five, so five minutes on is said rounded up.
    assert_eq!(right.message, "These settings are open until 5:06 pm.");
    assert_eq!(right.opens_now, None);
    phone.kept = phone.engine.save().unwrap();
    assert_eq!(phone.run(1).cover.kind, CoverKind::Nothing);

    // Kept: LiMo stopped and started again, still open; five minutes on,
    // guarded again.
    phone.crash();
    assert_eq!(phone.run(1).cover.kind, CoverKind::Nothing);
    assert_eq!(phone.run(5 * 60).cover.kind, CoverKind::Guard);

    // Bytes that are not text cannot be a PIN.
    assert!(matches!(phone.engine.try_guard_pin(vec![0xff], phone.wall), Err(EngineError::OutOfRange { .. })));
}

#[test]
fn the_accessibility_settings_are_guarded_and_nothing_before_setup_is_finished() {
    let mut phone = Phone::setting_up(at(17, 0));
    phone.front = app(SETTINGS, "Settings");
    phone.guarded = Some(GuardedPage::Accessibility);
    assert_eq!(phone.run(3).cover.kind, CoverKind::Nothing);
    assert!(phone.engine.finish_setup(phone.wall).unwrap());
    phone.kept = phone.engine.save().unwrap();
    let orders = phone.run(1);
    assert_eq!(orders.cover.guard.expect("the guard").page, GuardedPage::Accessibility);
}
